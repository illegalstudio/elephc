//! Purpose:
//! Resolves static namespace mappings and supported SPL registration patterns.
//! Prefixes eagerly loaded sources and inlines class files discovered by the AOT registry.
//!
//! Called from:
//! - `crate::pipeline::compile()`
//!
//! Key details:
//! - Runtime autoload callbacks cannot run in native binaries; supported rules are interpreted at compile time.
//! - Eager files execute before the entry program while class-triggered files splice before first use.
//! - `run_collecting_included` additionally surfaces the canonical path of every file the pass
//!   loaded, which `crate::opcache_prelude` bakes into the OPcache script manifest.
//! - Every loaded file is lexed with a fresh source identity that continues the compile's
//!   counter (its includes too), so its spans never equal another file's in span-keyed maps.

mod alias;
mod composer_files;
pub mod deferral;
pub mod deferral_memo;
mod dev_packages;
mod dynamic_contracts;
pub mod prelude_probe;
pub mod runtime_faults;
mod index;
mod interpret;
pub(crate) mod lazy_init;
mod registry;
mod rule;
mod walk;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub use dev_packages::phpunit_closure;
pub use registry::Registry;

use crate::errors::CompileError;
use crate::parser::ast::{BinOp, Expr, ExprKind, Program, Stmt, StmtKind};
use crate::span::Span;

use walk::{collect_declared_fqns, collect_reference_points};

/// Physical source paths for declarations introduced by static source loading.
#[derive(Debug, Default)]
pub struct DeclarationSourceFiles {
    pub class_likes: HashMap<String, String>,
    pub functions: HashMap<String, String>,
    /// Parsed source inputs, independent of whether PHP has entered their files.
    pub source_units: std::collections::BTreeMap<PathBuf, crate::resolver::SourceUnit>,
    /// Files whose conditional declarations were dropped rather than bound, mapped to the names
    /// they dropped. See `ResolveState::unbound_conditional_declaration_sources`.
    pub unbound_conditional_declaration_sources: HashMap<PathBuf, Vec<String>>,
    /// Names bound through the function-variant mechanism; subtracted from the dropped names.
    pub bound_conditional_declaration_names: HashSet<String>,
    /// Every function an eager Composer `files` entry declares, at any depth and in any
    /// namespace (`php_symbol_key` of the qualified name), mapped to that file. A body the
    /// backend cannot lower hands its file back to the runtime through this map, the way a class
    /// body hands back its class file (`autoload::deferral::BackendDeferral`).
    pub eager_function_files: HashMap<String, PathBuf>,
}

impl DeclarationSourceFiles {
    /// Merges declaration paths from another loaded source set.
    fn extend(&mut self, other: DeclarationSourceFiles) -> Result<(), CompileError> {
        for unit in other.source_units.into_values() {
            unit.insert_into(&mut self.source_units, Span::dummy())?;
        }
        self.class_likes.extend(other.class_likes);
        self.functions.extend(other.functions);
        self.eager_function_files.extend(other.eager_function_files);
        self.unbound_conditional_declaration_sources
            .extend(other.unbound_conditional_declaration_sources);
        self.bound_conditional_declaration_names
            .extend(other.bound_conditional_declaration_names);
        Ok(())
    }
}

/// Run the autoload pass over a fully resolver+name_resolver-processed
/// program. For every canonical class reference that isn't declared in
/// the program, look it up first in the static namespace index and then in
/// user-registered closure rules; parse the referenced file,
/// run resolver+name_resolver on it, and append. Iterate until stable.
///
/// This is the loaded-set-discarding wrapper over [`run_collecting_included`], kept for the
/// call sites that do not bake the OPcache script manifest (the `ir_lower` and
/// `tests/codegen/support` harnesses); only `crate::pipeline` takes the longer form.
#[allow(dead_code)] // Consumed by the test harnesses; `crate::pipeline` uses the collecting form.
pub fn run(
    program: Program,
    base_dir: &Path,
    registry: &Registry,
) -> Result<Program, CompileError> {
    run_collecting_included(program, base_dir, registry).map(|(program, _)| program)
}

/// Same as [`run`], but also returns the CANONICAL path of every source file this pass
/// pulled into the program, each exactly once:
/// - manifest-declared eager sources,
/// - every static-mapping or SPL-rule class file resolved by the fixpoint below,
/// - every `include`/`require` target those files themselves pull in (an autoloaded class
///   file that `require`s a helper compiles that helper into the binary too, so it is just
///   as much a cached script).
///
/// The first two come from the pass's own `included` set, which is already canonicalized
/// with `Path::canonicalize` — the SAME normalization `__FILE__` bakes
/// (`crate::magic_constants::file_pass`) — so the paths are directly comparable with
/// `crate::opcache_prelude::ScriptEntry::path`. The third comes from
/// `resolver::resolve_collecting_includes`, canonicalized identically.
///
/// Nested include paths are accumulated SEPARATELY from `included` rather than being
/// folded into it: `included` doubles as the "already autoloaded" guard, and seeding it
/// with include targets would change which files the fixpoint loads. Keeping them apart
/// makes this function's autoload behavior byte-identical to [`run`]'s.
///
/// The vector is SORTED so a build is byte-reproducible.
pub fn run_collecting_included(
    program: Program,
    base_dir: &Path,
    registry: &Registry,
) -> Result<(Program, Vec<PathBuf>), CompileError> {
    run_collecting_included_with_defines(program, base_dir, registry, &HashSet::new())
}

/// Runs autoload expansion while applying conditional symbols to every physical file loaded.
pub fn run_collecting_included_with_defines(
    program: Program,
    base_dir: &Path,
    registry: &Registry,
    defines: &HashSet<String>,
) -> Result<(Program, Vec<PathBuf>), CompileError> {
    run_collecting_included_with_defines_and_sources(program, base_dir, registry, defines)
        .map(|(program, files, _)| (program, files))
}

/// Runs autoload expansion and retains physical source paths for introduced declarations.
pub fn run_collecting_included_with_defines_and_sources(
    program: Program,
    base_dir: &Path,
    registry: &Registry,
    defines: &HashSet<String>,
) -> Result<(Program, Vec<PathBuf>, DeclarationSourceFiles), CompileError> {
    crate::compiler_stack::with_compiler_stack(|| {
        run_collecting_included_on_compiler_stack(program, base_dir, registry, defines)
    })
}

/// Expands autoloads after the public entry point has established the recursive stack budget.
fn run_collecting_included_on_compiler_stack(
    mut program: Program,
    base_dir: &Path,
    registry: &Registry,
    defines: &HashSet<String>,
) -> Result<(Program, Vec<PathBuf>, DeclarationSourceFiles), CompileError> {
    if registry.is_empty() {
        return Ok((program, Vec::new(), DeclarationSourceFiles::default()));
    }
    let mut included: HashSet<PathBuf> = HashSet::new();
    let mut nested_includes: HashSet<PathBuf> = HashSet::new();
    let mut declaration_sources = DeclarationSourceFiles::default();
    let mut deferred: Vec<PathBuf> = Vec::new();
    // -- prefix always-included files first --
    // Eager source manifests declare files that must always be included. Preserve
    // declaration order so their top-level statements execute before the entry program.
    let mut prefix: Program = Vec::new();
    // Accumulated from every eager file that was READ, including the ones declined below. A
    // declined file is not spliced, so reading this off `prefix` afterwards would miss exactly
    // the polyfills whose functions most need the global fallback — measured: with the two
    // changes in place and this read from `prefix`, Symfony's console died on
    // `symfony\component\varexporter\deepclone_to_array()`, a function declared by a declined
    // eager file that the RUNTIME goes on to declare perfectly well.
    let mut eager_globals: HashSet<String> = HashSet::new();
    // An eager file an earlier round of this build handed back — the backend could not lower one
    // of its functions — is declined whole here, exactly as a half-taken one is below.
    let handed_back: HashSet<PathBuf> = deferral::inherited_deferred_files()
        .into_iter()
        .map(|path| path.canonicalize().unwrap_or(path))
        .collect();
    for path in registry.always_included_files() {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if handed_back.contains(&canonical) {
            continue;
        }
        if included.insert(canonical.clone()) {
            // An eager file the compiler cannot parse is handed to the runtime whole, exactly as
            // a half-taken one is below: php requires it at startup and so will the program.
            let (loaded, loaded_includes, loaded_sources) =
                match load_autoloaded_file(&canonical, base_dir, defines) {
                    Ok(loaded) => loaded,
                    Err(AutoloadFileError::Unparsed(error)) => {
                        report_deferred_class_file(&canonical, &error);
                        included.remove(&canonical);
                        continue;
                    }
                    Err(error) => return Err(error.into_compile_error()),
                };
            // TAKE IT WHOLE OR NOT AT ALL. A conditional declaration under this eager file was
            // dropped instead of bound AND nothing in its subtree bound that name, so splicing
            // what is left would publish a file the compiler only half-took: the runtime skips
            // every file the compiler claims, so the function would be declared by nobody and the
            // first call to it would fail with `Call to undefined function`.
            //
            // MEASURED on Symfony `--web`, four builds, two with this rule and two without. The
            // rule is what makes `GET /` render: without it the route dies on
            // `twig\extension\mb_strtoupper` and the assembly is ~1 426 109 37x bytes; with it
            // all eight prod routes answer byte-identically to `php -S` and the assembly is
            // ~1 426 081 0xx. `polyfill-mbstring/bootstrap.php` is the file in question — it
            // delegates to a `bootstrap80.php` of nothing but `if (!function_exists('mb_…'))`
            // declarations, every one of which was dropped while the file was still reported as
            // included. Leaving it out costs only that it runs interpreted, as php does anyway.
            //
            // THE SUBTRACTION IS LOAD-BEARING. A version guard declares the same function in both
            // branches; only the live one is bound, and condemning a file on the dead branch's
            // drop alone would be wrong. `tests/eager_file_polyfill_tests.rs` holds both shapes.
            let bound = &loaded_sources.bound_conditional_declaration_names;
            // TWO ways the compiler can fail to take an eager file WHOLE, and they need different
            // questions because they have different causes.
            //
            // 1. A conditional declaration under it was dropped and bound nowhere. That is
            //    `symfony/polyfill-mbstring`, whose `return require …` IS expanded and whose
            //    `if (!function_exists('mb_…'))` declarations are then stripped out.
            //
            // 2. An include in it was never expanded at all, so the subtree was never read and
            //    reports neither drops nor bindings. That is `symfony/polyfill-deepclone`, whose
            //    `if (\PHP_VERSION_ID >= 80100) { require __DIR__.'/bootstrap81.php'; }` stays a
            //    runtime include — measured `dropped=[] bound=0`, and `deepclone_to_array` absent
            //    from EVERY compile phase while the runtime skipped the parent on its claim, so
            //    the function was declared by nobody.
            //
            // Declining hands the file to the runtime entirely: it is not spliced either, so its
            // top-level effects run exactly once, as php runs them.
            let lost = loaded_sources
                .unbound_conditional_declaration_sources
                .values()
                .flatten()
                .any(|name| !bound.contains(name))
                || contains_unexpanded_include(&loaded);
            eager_globals.extend(eager_global_function_keys(&loaded));
            // The DROPPED names too. A two-hop polyfill (`bootstrap.php` -> `require
            // bootstrap80.php`) has its declarations stripped out of `loaded` before this point,
            // so walking the statements finds nothing — and those are precisely the functions the
            // RUNTIME will declare, which is what the global fallback is for. Measured on
            // `symfony/polyfill-deepclone`: `deepclone_to_array` is dropped, declared at run time,
            // and called bare from `namespace Symfony\Component\VarExporter`.
            eager_globals.extend(
                loaded_sources
                    .unbound_conditional_declaration_sources
                    .values()
                    .flatten()
                    .filter(|name| !name.trim_start_matches('\\').contains('\\'))
                    .map(|name| crate::names::php_symbol_key(name.trim_start_matches('\\'))),
            );
            // `ELEPHC_EAGER_TRACE=1` reports what the compiler actually TOOK from each eager
            // file, which is the only way to tell the three shapes of a half-taken file apart.
            // Reading it: `dropped` are conditional declarations the gate discarded, `bound` are
            // the ones the function-variant mechanism kept, `globals` are the global functions
            // still present at any depth, and `nested` are the includes that were expanded.
            //
            //   mbstring   lost=true  dropped=["mb_…"]              -> declined, runtime owns it
            //   deepclone  lost=false dropped=[] bound=0 globals={} nested=["bootstrap81.php"]
            //              -> the subtree WAS read and contributed nothing, and nothing was
            //                 reported as dropped either. That combination is not explained by
            //                 any of the three known shapes and is why `deepclone_to_array()` is
            //                 declared by nobody.
            if std::env::var("ELEPHC_EAGER_TRACE").is_ok() {
                eprintln!(
                    "[elephc-eager] file={} lost={lost} dropped={:?} bound={} globals={:?} nested={:?}",
                    canonical.display(),
                    loaded_sources
                        .unbound_conditional_declaration_sources
                        .values()
                        .flatten()
                        .collect::<Vec<_>>(),
                    loaded_sources.bound_conditional_declaration_names.len(),
                    eager_global_function_keys(&loaded),
                    loaded_includes
                        .iter()
                        .map(|path| path
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_default())
                        .collect::<Vec<_>>(),
                );
            }
            if lost {
                included.remove(&canonical);
                continue;
            }
            for key in eager_function_keys(&loaded) {
                declaration_sources
                    .eager_function_files
                    .insert(key, canonical.clone());
            }
            nested_includes.extend(loaded_includes);
            declaration_sources.extend(loaded_sources)?;
            prefix.extend(loaded);
        }
    }
    // Published from the statements as READ, not from `declaration_sources.functions`: a
    // polyfill's declaration sits inside `if (!function_exists(…))`, and the declaration-source
    // walk records only file-scope ones, so that map is empty for exactly this shape.
    crate::eager_globals::set(eager_globals);
    if !prefix.is_empty() {
        prefix.extend(program);
        program = prefix;
    }
    // Every GLOBAL function the eager files just declared, published before a single class file is
    // resolved. That ordering is what makes PHP's global fallback expressible here: a class file
    // resolved later calls `trigger_deprecation()` bare from inside its own namespace, and the
    // resolver can now answer it from a DECLARATION instead of from a hand-maintained allow-list
    // (`name_resolver::canonical_compat_prelude_function_name`, which has been caught missing a
    // name three times). A namespaced declaration is excluded: PHP falls back to the global
    // namespace only, never to another one.

    loop {
        let mut declared = collect_declared_fqns(&program);
        seed_builtin_declared_fqns(&mut declared);
        let mut reference_points = collect_reference_points(&program);
        reference_points.extend(
            dynamic_contracts::candidates(&program)
                .into_iter()
                .map(|name| (0, name)),
        );
        let mut insertions: Vec<(usize, Program)> = Vec::new();
        // Class names are case-insensitive: `class_exists("demo\\thing")` names the declared
        // `Demo\Thing`, and an exact-case lookup loaded its file a second time ("Duplicate class
        // declaration: Demo\Thing"). Refreshed after every bundle, which also extends `declared`.
        let mut declared_keys: HashSet<String> =
            declared.iter().map(|name| crate::names::php_symbol_key(name)).collect();
        for (stmt_idx, fqn) in reference_points {
            if declared.contains(&fqn) || declared_keys.contains(&crate::names::php_symbol_key(&fqn)) {
                continue;
            }
            if let Some(path) = resolve_class(&fqn, registry) {
                let canonical = path.canonicalize().unwrap_or(path);
                if included.insert(canonical.clone()) {
                    let bundle = load_autoloaded_bundle(
                        &fqn,
                        &canonical,
                        base_dir,
                        defines,
                        registry,
                        &mut declared,
                        &mut included,
                        &mut nested_includes,
                        &mut declaration_sources,
                        &mut deferred,
                        0,
                        Some(&program[stmt_idx]),
                    )?;
                    if !bundle.is_empty() {
                        // What this bundle declares is declared from here on, not from the next
                        // pass: php's autoloader is never asked for a class a file it already
                        // loaded declared. MEASURED with `nikic/php-parser`: `Int_.php` ends with
                        // `class_alias(Int_::class, LNumber::class)`, so a later `LNumber`
                        // reference in the same pass must not load `LNumber.php` — whose plain
                        // `require __DIR__.'/Int_.php'` spliced `Int_` a second time and failed
                        // the build with "Duplicate class declaration".
                        // Everything `load_autoloaded_bundle` added to `declared` is declared by
                        // the bundle it returned, so the keys grow by the bundle's names alone;
                        // rebuilding them from the whole set per bundle was quadratic in files.
                        let bundle_names = collect_declared_fqns(&bundle);
                        declared_keys.extend(
                            bundle_names.iter().map(|name| crate::names::php_symbol_key(name)),
                        );
                        declared.extend(bundle_names);
                        insertions.push((stmt_idx, bundle));
                    }
                }
            }
        }
        if insertions.is_empty() {
            break;
        }
        let mut offset = 0usize;
        for (stmt_idx, loaded) in insertions {
            let insert_at = stmt_idx + offset;
            offset += loaded.len();
            program.splice(insert_at..insert_at, loaded);
        }
    }

    included.extend(nested_includes);
    // A deferred class file was never read into the program, so it must not be reported as
    // compile-time included: the runtime primes the include guard of every reported file, and
    // the autoloader's `include` would then be skipped and the class never declared.
    for path in &deferred {
        included.remove(path);
    }
    let mut loaded_files: Vec<PathBuf> = included.into_iter().collect();
    loaded_files.sort();
    Ok((program, loaded_files, declaration_sources))
}





/// Returns whether any include in these statements is still a RUNTIME include.
///
/// The resolver expands what it can reach statically. What it leaves behind is a file the
/// compiled program will not have read, so an eager entry holding one has not been taken whole —
/// see the call site for the measurement.
fn contains_unexpanded_include(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|stmt| {
        matches!(stmt.kind, StmtKind::Include { .. })
            || eager_nested_bodies(stmt)
                .into_iter()
                .any(contains_unexpanded_include)
    })
}

/// Collects the GLOBAL functions an eager file declares, at any statement depth.
///
/// Depth matters: every polyfill writes `if (!function_exists('f')) { function f() {…} }`, so a
/// file-scope-only walk finds nothing. A namespaced declaration is skipped — PHP's bare-call
/// fallback reaches the global namespace and no other.
fn eager_global_function_keys(stmts: &[Stmt]) -> HashSet<String> {
    fn walk(stmts: &[Stmt], out: &mut HashSet<String>) {
        for stmt in stmts {
            if let StmtKind::FunctionDecl { name, .. } = &stmt.kind {
                let bare = name.trim_start_matches('\\');
                if !bare.contains('\\') {
                    out.insert(crate::names::php_symbol_key(bare));
                }
            }
            for nested in eager_nested_bodies(stmt) {
                walk(nested, out);
            }
        }
    }
    let mut out = HashSet::new();
    walk(stmts, &mut out);
    out
}

/// Collects EVERY function an eager file declares, at any depth and in any namespace, keyed like
/// the symbol tables (`php_symbol_key`, no leading `\`). Unlike [`eager_global_function_keys`]
/// this keeps namespaced names: it answers "which file declared this body", not "which names the
/// global fallback may reach".
fn eager_function_keys(stmts: &[Stmt]) -> Vec<String> {
    fn walk(stmts: &[Stmt], out: &mut Vec<String>) {
        for stmt in stmts {
            if let StmtKind::FunctionDecl { name, .. } = &stmt.kind {
                out.push(crate::names::php_symbol_key(name.trim_start_matches('\\')));
            }
            for nested in eager_nested_bodies(stmt) {
                walk(nested, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(stmts, &mut out);
    out
}

/// Returns the statement lists nested inside one statement, for [`eager_global_function_keys`].
fn eager_nested_bodies(stmt: &Stmt) -> Vec<&[Stmt]> {
    let mut bodies: Vec<&[Stmt]> = Vec::new();
    match &stmt.kind {
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            bodies.push(then_body);
            bodies.extend(elseif_clauses.iter().map(|(_, body)| body.as_slice()));
            if let Some(body) = else_body {
                bodies.push(body);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            bodies.push(then_body);
            if let Some(body) = else_body {
                bodies.push(body);
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::Synthetic(body) => bodies.push(body),
        StmtKind::Switch { cases, default, .. } => {
            bodies.extend(cases.iter().map(|(_, body)| body.as_slice()));
            if let Some(body) = default {
                bodies.push(body);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            bodies.push(try_body);
            bodies.extend(catches.iter().map(|catch| catch.body.as_slice()));
            if let Some(body) = finally_body {
                bodies.push(body);
            }
        }
        _ => {}
    }
    bodies
}

/// The project root a build's autoload index is read from: the nearest ancestor of `entry_dir`
/// with a supported autoload manifest.
pub fn project_root(entry_dir: &Path) -> Option<PathBuf> {
    index::nearest_autoload_manifest_root(entry_dir)
}

/// Returns the class-like names the program only ever hands to an existence probe.
///
/// See [`walk::probe_only_class_names`]: these are the classes a closed-world build carries
/// solely so `class_exists()` can answer, and which `class_exists($name, false)` must therefore
/// still report as NOT LOADED.
pub fn probe_only_class_names(program: &Program) -> std::collections::HashSet<String> {
    walk::probe_only_class_names(program)
}

/// Records what the AUTOLOAD PASS already did, so the runtime does not try to do it again.
///
/// Two facts, both derived from the same pass and both needed by the generated program:
///
/// - `preincluded_sources`: the compiler opened these files and spliced their declarations in,
///   which is the inclusion php's autoloader would have performed. An `include_once` reaching
///   one of them at runtime must answer "already included" instead of redeclaring everything.
/// - `deferred_class_loads`: the classes among them that the program only ever hands to an
///   existence probe. php would never have loaded those, so `class_exists($n, false)` has to
///   keep reporting them as not loaded until a probe with autoloading asks for one.
///
/// Every compile path has to call this -- the CLI pipeline and the test harness alike -- or the
/// two behave differently on the same program.
pub fn record_compile_time_inclusions(
    module: &mut crate::ir::Module,
    program: &Program,
    autoloaded_files: &[std::path::PathBuf],
) {
    module.preincluded_sources = autoloaded_files
        .iter()
        .map(|path| path.canonicalize().unwrap_or_else(|_| path.clone()))
        .collect();
    let probe_only: std::collections::HashSet<String> = probe_only_class_names(program)
        .into_iter()
        .map(|name| name.to_ascii_lowercase())
        .collect();
    if probe_only.is_empty() {
        return;
    }
    let autoloaded: std::collections::HashSet<std::path::PathBuf> =
        module.preincluded_sources.iter().cloned().collect();
    module.deferred_class_loads = module
        .declared_class_source_files
        .iter()
        .filter(|(name, file)| {
            // Both sides are canonicalized: a declaration path can arrive in the `/var/...`
            // spelling while the autoload pass reports `/private/var/...` for the same file,
            // and comparing them raw silently matches nothing.
            let declared = std::path::PathBuf::from(file);
            let declared = declared.canonicalize().unwrap_or(declared);
            probe_only.contains(&name.trim_start_matches('\\').to_ascii_lowercase())
                && autoloaded.contains(&declared)
        })
        .map(|(name, _)| name.trim_start_matches('\\').to_ascii_lowercase())
        .collect();
}

/// Lower any top-level literal `class_alias()` calls left after another
/// expansion pass, such as resolver includes or autoloaded files.
///
/// Returns a `Result` for the callers that predate the dropped `autoload = false` diagnostic: a
/// literal `false` is now collected statically (Symfony's generated container relies on it), so
/// this never fails.
pub fn collect_aliases(program: Program) -> Result<Program, CompileError> {
    Ok(alias::collect_aliases(program))
}

/// Collects static class-string aliases after name resolution and autoload expansion.
pub(crate) fn collect_resolved_aliases(program: Program) -> Program {
    alias::collect_resolved_aliases(program)
}

/// Returns the canonical class pair when alias arguments are statically resolvable.
pub(crate) fn resolved_class_alias_args(args: &[crate::parser::ast::Expr]) -> Option<(String, String)> {
    alias::resolved_class_alias_args(args)
}

/// Inserts PHP's built-in class-like names into `declared` so that references
/// to types like `Exception`, `stdClass`, and `Iterator` are never treated as
/// autoload demands. Called at the start of each autoload iteration.
fn seed_builtin_declared_fqns(declared: &mut HashSet<String>) {
    // Every catalogued builtin class-like (`Exception`, `stdClass`, `Iterator`, `PDO`, ...)
    // is seeded into the declared FQN set so references to it are never autoload demands.
    for name in crate::types::builtin_classes::builtin_class_like_names() {
        declared.insert((*name).to_string());
    }
}

/// Tries the resolution chain in order: static namespace mappings first, then
/// each user-registered closure rule. Returns the first rule that produces a
/// path matching an existing file on disk.
fn resolve_class(fqn: &str, registry: &Registry) -> Option<PathBuf> {
    if let Some(path) = registry.psr4().lookup(fqn) {
        return Some(path.to_path_buf());
    }
    for rule in registry.rules() {
        if let Some(path) = interpret::resolve(rule, fqn) {
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

/// Returns whether the source selected for an autoload demand can bind that class-like symbol.
///
/// PHP loads a candidate file before it binds the requested declaration. A class whose direct
/// parent, interface, or used trait cannot itself be found therefore remains absent; code guarded
/// by an existence probe can continue, while an actually reached construction still fails through
/// the compiler's normal absent-class path. Keeping such a declaration out of the closed world also
/// prevents an eagerly inspected but dormant file from turning a runtime-conditional failure into a
/// whole-program schema error.
fn autoload_target_can_bind(
    program: &Program,
    target: &str,
    declared: &HashSet<String>,
    registry: &Registry,
) -> bool {
    let Some(dependencies) = direct_binding_dependencies(program, target) else {
        return true;
    };
    let local = collect_declared_fqns(program)
        .into_iter()
        .map(|name| crate::names::php_symbol_key(&name))
        .collect::<HashSet<_>>();
    let available = declared
        .iter()
        .map(|name| crate::names::php_symbol_key(name))
        .collect::<HashSet<_>>();

    dependencies.into_iter().all(|dependency| {
        let key = crate::names::php_symbol_key(&dependency);
        local.contains(&key)
            || available.contains(&key)
            || resolve_class(&dependency, registry).is_some()
    })
}

/// Returns the class-like a file's top-level `if (!interface_exists(X::class)) { throw ...; }`
/// guard demands when nothing can supply it: not declared by the file, not already known, and not
/// resolvable by the autoloader.
///
/// php runs such a guard when the autoloader first includes the file, so loading it THROWS, and
/// only code that actually asks for the class ever sees that. Symfony's `HttpClient::create()`
/// names `AmpHttpClient` behind `class_exists(AmpRequest::class)`; `AmpHttpClient.php` opens with
/// `if (!interface_exists(DelegateHttpClient::class)) { throw new \LogicException(...); }`.
/// Compiled into the program, its file-scope statements ran at boot and every request died on
/// "the amphp/http-client package is not installed" although nothing ever constructed the client.
/// Such a file is left to the runtime autoloader, which throws the same exception at the same
/// moment php would.
fn unsatisfiable_file_scope_guard(
    program: &Program,
    declared: &HashSet<String>,
    registry: &Registry,
) -> Option<String> {
    let local = collect_declared_fqns(program)
        .into_iter()
        .map(|name| crate::names::php_symbol_key(&name))
        .collect::<HashSet<_>>();
    let available = |name: &str| {
        let key = crate::names::php_symbol_key(name);
        local.contains(&key)
            || declared
                .iter()
                .any(|declared| crate::names::php_symbol_key(declared) == key)
            || resolve_class(name, registry).is_some()
    };
    file_scope_guard_classes(program)
        .into_iter()
        .find(|name| !available(name))
}

/// Collects the class-likes named by top-level `if (!class_exists(X::class)) { throw ...; }`
/// guards (also `interface_exists`, `trait_exists`, `enum_exists`, and a literal class name),
/// looking through namespace blocks. Only a guard whose body throws and that has no other branch
/// counts: anything else is ordinary conditional code.
fn file_scope_guard_classes(program: &[Stmt]) -> Vec<String> {
    let mut out = Vec::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::NamespaceBlock { body, .. } => out.extend(file_scope_guard_classes(body)),
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } if elseif_clauses.is_empty()
                && else_body.is_none()
                && then_body.first().is_some_and(stmt_throws) =>
            {
                if let Some(name) = negated_existence_probe(condition) {
                    out.push(name);
                }
            }
            _ => {}
        }
    }
    out
}

/// Returns true for a `throw` statement.
fn stmt_throws(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Throw(_) => true,
        StmtKind::ExprStmt(expr) => matches!(expr.kind, ExprKind::Throw(_)),
        _ => false,
    }
}

/// Returns `X` for `!class_exists(X::class)` and its siblings, when the probe autoloads.
fn negated_existence_probe(condition: &Expr) -> Option<String> {
    let ExprKind::Not(inner) = &condition.kind else {
        return None;
    };
    let ExprKind::FunctionCall { name, args } = &inner.kind else {
        return None;
    };
    let function = name.as_canonical().trim_start_matches('\\').to_ascii_lowercase();
    if !matches!(
        function.as_str(),
        "class_exists" | "interface_exists" | "trait_exists" | "enum_exists"
    ) {
        return None;
    }
    // `class_exists(X::class, false)` does not run the autoloader, and whether X exists then
    // depends on load order the compiler does not model.
    let autoloads = match args.get(1).map(|arg| &arg.kind) {
        None => true,
        Some(ExprKind::BoolLiteral(value)) => *value,
        Some(_) => false,
    };
    if !autoloads {
        return None;
    }
    let class = match &args.first()?.kind {
        ExprKind::ClassConstant {
            receiver: crate::parser::ast::StaticReceiver::Named(name),
        } => name.as_canonical().trim_start_matches('\\').to_string(),
        ExprKind::StringLiteral(name) => name.trim_start_matches('\\').to_string(),
        _ => return None,
    };
    (!class.is_empty()).then_some(class)
}

/// Reports a class file whose file-scope guard would throw, left to the runtime autoloader.
///
/// A note, not a warning: the program behaves exactly as php does -- the class can never be
/// loaded, and asking for it throws the guard's exception at the moment php would. Symfony's
/// `AmpHttpClient` without amphp installed is the usual case, in every application that ships
/// `symfony/http-client`.
fn report_throwing_class_file(path: &Path, missing: &str) {
    crate::errors::report_note(&format!(
        "autoload: '{}' is not compiled (its file-scope guard throws: '{}' is not available); \
         its classes are loaded at run time by the autoloader instead",
        path.display(),
        missing
    ));
}

/// Reports a class file left to the runtime because its file-scope code calls something and
/// nothing reaches the class at file scope.
///
/// php runs that code when the autoloader first includes the file, i.e. when something asks for
/// one of its classes. Spliced among the declarations it ran at boot: psysh's
/// `Hoa/ConsoleWindow.php` ends with `Console::advancedInteraction();`, which shells out to
/// `stty`, so every Laravel artisan command died at startup although none of them touches the
/// console window. A class named by executable file-scope code keeps its splice point there.
fn report_effectful_class_file(path: &Path, call: &str) {
    crate::errors::report_warning(&crate::errors::CompileWarning::new(
        Span::dummy(),
        &format!(
            "autoload: '{}' is not compiled (its file-scope code calls `{}`, which php runs only \
             when the class is first loaded); its classes are loaded at run time by the \
             autoloader instead",
            path.display(),
            call
        ),
    ));
}

/// Reports a class file left to the runtime because it declares a class-like conditionally.
fn report_conditional_class_file(path: &Path, class_like: &str) {
    crate::errors::report_warning(&crate::errors::CompileWarning::new(
        Span::dummy(),
        &format!(
            "autoload: '{}' is not compiled (it declares '{}' inside a run-time condition); its \
             classes are loaded at run time by the autoloader instead",
            path.display(),
            class_like
        ),
    ));
}

/// Finds the direct class-like dependencies of the requested declaration in one loaded source.
fn direct_binding_dependencies(program: &Program, target: &str) -> Option<Vec<String>> {
    let target_key = crate::names::php_symbol_key(target.trim_start_matches('\\'));
    for stmt in program {
        match &stmt.kind {
            StmtKind::ClassDecl {
                name,
                extends,
                implements,
                trait_uses,
                ..
            } if crate::names::php_symbol_key(name.trim_start_matches('\\')) == target_key => {
                let mut dependencies = Vec::new();
                if let Some(parent) = extends {
                    dependencies.push(parent.as_canonical().trim_start_matches('\\').to_string());
                }
                dependencies.extend(
                    implements
                        .iter()
                        .map(|name| name.as_canonical().trim_start_matches('\\').to_string()),
                );
                dependencies.extend(trait_uses.iter().flat_map(|trait_use| {
                    trait_use.trait_names.iter().map(|name| {
                        name.as_canonical().trim_start_matches('\\').to_string()
                    })
                }));
                return Some(dependencies);
            }
            StmtKind::InterfaceDecl { name, extends, .. }
                if crate::names::php_symbol_key(name.trim_start_matches('\\')) == target_key =>
            {
                return Some(
                    extends
                        .iter()
                        .map(|name| name.as_canonical().trim_start_matches('\\').to_string())
                        .collect(),
                );
            }
            StmtKind::TraitDecl {
                name, trait_uses, ..
            } if crate::names::php_symbol_key(name.trim_start_matches('\\')) == target_key => {
                return Some(
                    trait_uses
                        .iter()
                        .flat_map(|trait_use| {
                            trait_use.trait_names.iter().map(|name| {
                                name.as_canonical().trim_start_matches('\\').to_string()
                            })
                        })
                        .collect(),
                );
            }
            StmtKind::EnumDecl {
                name, implements, ..
            } if crate::names::php_symbol_key(name.trim_start_matches('\\')) == target_key => {
                return Some(
                    implements
                        .iter()
                        .map(|name| name.as_canonical().trim_start_matches('\\').to_string())
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => {
                if let Some(dependencies) = direct_binding_dependencies(body, target) {
                    return Some(dependencies);
                }
            }
            _ => {}
        }
    }
    None
}

/// How deep one autoload demand may chase its own dependencies before the outer fixpoint loop
/// takes over. Real inheritance chains are a dozen links at most; the cap only stops a pathological
/// or cyclic graph from overflowing the stack.
const AUTOLOAD_BUNDLE_MAX_DEPTH: usize = 64;

/// Loads one autoloaded file together with the files its own file-scope execution demands,
/// ordered dependency-first.
///
/// PHP's autoloader is DEPTH-FIRST: asking for `AsciiSlugger` runs the loader again for
/// `LocaleAwareInterface` before the class binds, and again for whatever that interface needs.
/// Appending each discovered file at its own first reference point instead is breadth-first, and
/// it can place a dependency AFTER the file that demanded it. `symfony/string`'s `AsciiSlugger.php`
/// guards its whole body with `if (!interface_exists(LocaleAwareInterface::class)) { throw ... }`;
/// the interface landed nine statements too late and the compiled program threw
/// "the symfony/translation-contracts package is not installed" before the entry file's first
/// statement ran.
///
/// Returns an empty program when the target cannot bind here, which is the caller's signal to
/// insert nothing — the path stays in `included` either way, exactly as before.
///
/// A class file the compiler cannot PARSE is not a build failure: it is pushed onto `deferred`
/// and left to the runtime autoloader, which is what php does with every class file — it
/// parses one only when something asks for the class. The caller takes deferred paths back out
/// of the compile-time inclusion set, or the runtime would skip the include as already done.
#[allow(clippy::too_many_arguments)]
fn load_autoloaded_bundle(
    fqn: &str,
    canonical: &Path,
    base_dir: &Path,
    defines: &HashSet<String>,
    registry: &Registry,
    declared: &mut HashSet<String>,
    included: &mut HashSet<PathBuf>,
    nested_includes: &mut HashSet<PathBuf>,
    declaration_sources: &mut DeclarationSourceFiles,
    deferred: &mut Vec<PathBuf>,
    depth: usize,
    reference: Option<&Stmt>,
) -> Result<Program, CompileError> {
    trace_autoload_load(fqn, canonical, depth);
    let (loaded, loaded_includes, loaded_sources) =
        match load_autoloaded_file(canonical, base_dir, defines) {
            Ok(loaded) => loaded,
            Err(AutoloadFileError::Unparsed(error)) => {
                report_deferred_class_file(canonical, &error);
                deferred.push(canonical.to_path_buf());
                return Ok(Vec::new());
            }
            Err(AutoloadFileError::Fatal(error)) => return Err(error),
        };
    let mut loaded =
        fold_file_scope_existence_conditions(loaded, declared, registry, base_dir, defines);
    // A closed world has no autoloader to leave a conditional declaration to: the class is
    // compiled and its condition checked on first use (`lazy_init`).
    let guarded_declarations = crate::closed_world::is_enabled()
        && lazy_init::unconditional_class_declarations(&mut loaded);
    // Before the bind check: a class the file declares only under a run-time condition cannot be
    // taken by the compiled world at all, and keeping the file claimed would make the runtime skip
    // the one include that declares it. See `walk::first_conditionally_declared_class_like`.
    if let Some(conditional) = walk::first_conditionally_declared_class_like(&loaded) {
        report_conditional_class_file(canonical, &conditional);
        deferred.push(canonical.to_path_buf());
        return Ok(Vec::new());
    }
    if !autoload_target_can_bind(&loaded, fqn, declared, registry) {
        return Ok(Vec::new());
    }
    if let Some(missing) = unsatisfiable_file_scope_guard(&loaded, declared, registry) {
        report_throwing_class_file(canonical, &missing);
        deferred.push(canonical.to_path_buf());
        return Ok(Vec::new());
    }
    // A closed world cannot leave the file to the autoloader for its deprecation notice; see
    // `walk::strip_file_scope_deprecations`.
    let mut loaded = loaded;
    if crate::closed_world::is_enabled() && walk::strip_file_scope_deprecations(&mut loaded) {
        crate::errors::report_note(&format!(
            "autoload: '{}' is compiled without its file-scope `trigger_deprecation()`, which php \
             runs when the class is first loaded",
            canonical.display()
        ));
    }
    // Reached only from function or method bodies, the file would be spliced among the
    // declarations and its file-scope code would run at boot; php runs it when the class is
    // first asked for, which may be never. See `report_effectful_class_file`.
    // A closed world has no autoloader to hand such a file to, and splicing it would run its
    // file-scope code at boot: that code becomes an initializer its classes' uses call, which is
    // when php runs it (`lazy_init`). A file loaded as another's parent is no exception.
    if crate::closed_world::is_enabled() {
        if let Some(call) = walk::first_file_scope_effect_call(&loaded)
            .or_else(|| guarded_declarations.then(|| "its declaration guard".to_string()))
        {
            if let Some(hoisted) = lazy_init::hoist_file_scope_effects(loaded.clone()) {
                crate::errors::report_note(&format!(
                    "autoload: '{}' is compiled; its file-scope code (`{}`) runs when one of its \
                     classes is first used, as php runs it when the class is first loaded",
                    canonical.display(),
                    call
                ));
                loaded = hoisted;
            }
        }
    }
    if let Some(reference) = reference {
        if let Some(call) = walk::first_file_scope_effect_call(&loaded) {
            if !walk::stmt_references_executably(reference, fqn) {
                report_effectful_class_file(canonical, &call);
                deferred.push(canonical.to_path_buf());
                return Ok(Vec::new());
            }
        }
    }

    // What this file declares ITSELF — directly, or through a `require` the resolver already
    // spliced into it — satisfies its own dependencies. MEASURED with `nikic/php-parser`:
    // `Stmt/UseUse.php` requires `../UseItem.php` and then declares
    // `if (false) { class UseUse extends UseItem {} }`, and walking that dependency loaded
    // `UseItem.php` a second time: "Duplicate class declaration: PhpParser\Node\UseItem".
    declared.extend(collect_declared_fqns(&loaded));
    let mut bundle: Program = Vec::new();
    if depth < AUTOLOAD_BUNDLE_MAX_DEPTH {
        let mut seen: HashSet<String> = HashSet::new();
        for dependency in walk::collect_file_scope_dependencies(&loaded) {
            if declared.contains(&dependency) || !seen.insert(dependency.clone()) {
                continue;
            }
            let Some(path) = resolve_class(&dependency, registry) else {
                continue;
            };
            let dependency_path = path.canonicalize().unwrap_or(path);
            // Inserting before the recursion is what breaks a dependency cycle: the second
            // visit finds the path already claimed and stops.
            if !included.insert(dependency_path.clone()) {
                continue;
            }
            let dependency_bundle = load_autoloaded_bundle(
                &dependency,
                &dependency_path,
                base_dir,
                defines,
                registry,
                declared,
                included,
                nested_includes,
                declaration_sources,
                deferred,
                depth + 1,
                None,
            )?;
            // Same rule as the caller's loop: a later dependency the one just loaded already
            // declared (a `class_alias()` at its end, say) must not load its own file.
            declared.extend(collect_declared_fqns(&dependency_bundle));
            bundle.extend(dependency_bundle);
        }
    }

    nested_includes.extend(loaded_includes);
    declaration_sources.extend(loaded_sources)?;
    bundle.extend(loaded);
    Ok(bundle)
}

/// Reports one class file the walker is about to load and the class that asked for it, when
/// `ELEPHC_AUTOLOAD_TRACE` is set.
///
/// The walker's decisions are otherwise invisible: a file loaded twice, or loaded for a class
/// something already declared, shows up only as a "Duplicate class declaration" with no file, and
/// the trace is what names the reference that caused it. `depth` is 0 for a reference in the
/// program and grows along a bundle's dependency chain.
fn trace_autoload_load(fqn: &str, canonical: &Path, depth: usize) {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ENABLED.get_or_init(|| std::env::var_os("ELEPHC_AUTOLOAD_TRACE").is_some()) {
        eprintln!("[elephc-autoload] depth={depth} class={fqn} file={}", canonical.display());
    }
}

/// Why one autoloaded file could not be taken into the compiled program.
#[derive(Debug)]
enum AutoloadFileError {
    /// The compiler's lexer or parser refused the file. php only parses a class file when
    /// something asks for the class, so this is not a build failure: the file is left to the
    /// runtime autoloader.
    Unparsed(CompileError),
    /// Anything else, which still stops the build.
    Fatal(CompileError),
}

impl From<CompileError> for AutoloadFileError {
    /// Every error past parsing keeps stopping the build.
    fn from(error: CompileError) -> Self {
        AutoloadFileError::Fatal(error)
    }
}

impl AutoloadFileError {
    /// Hands back the underlying diagnostic, for callers that treat both kinds alike.
    fn into_compile_error(self) -> CompileError {
        match self {
            AutoloadFileError::Unparsed(error) | AutoloadFileError::Fatal(error) => error,
        }
    }
}

/// Reports a class file the compiler could not parse and has left to the runtime autoloader.
fn report_deferred_class_file(path: &Path, error: &CompileError) {
    let at = if error.span == Span::dummy() {
        String::new()
    } else {
        format!(":{}:{}", error.span.line, error.span.col)
    };
    crate::errors::report_warning(&crate::errors::CompileWarning::new(
        Span::dummy(),
        &format!(
            "autoload: '{}'{} could not be parsed ({}); its classes are not compiled and are \
             loaded at run time by the autoloader instead",
            path.display(),
            at,
            error.message
        ),
    ));
}

/// Load, parse, and resolve a single autoloaded PHP file, returning its statements plus the
/// canonical paths of every `include`/`require` target the file itself pulled in (surfaced for
/// the OPcache script manifest — see [`run_collecting_included`]).
fn load_autoloaded_file(
    path: &Path,
    base_dir: &Path,
    defines: &HashSet<String>,
) -> Result<(Program, Vec<PathBuf>, DeclarationSourceFiles), AutoloadFileError> {
    let content = crate::source::read_physical_source(path).map_err(|e| {
        CompileError::new(
            Span::dummy(),
            &format!("Autoload: cannot read '{}': {}", path.display(), e),
        )
    })?;
    let file_label = path.display().to_string();
    let source_mode = crate::source::SourceMode::from_path(path);
    // Its own source identity, continuing the compile's counter. Lexed with the entry file's
    // identity 0, a statement here shared its span with the statement at the same line and
    // column of the entry and of every other autoloaded file, so span-keyed checker results (a
    // readonly write guard, a builtin call type, a binding decision) leaked between them: one
    // class got another class's readonly guard (issue #1445). It is registered under the
    // canonical path, so a compiled frame, a warning and a Throwable created in this class
    // file name THIS file rather than the program's entry file.
    let source_id = Span::fresh_source_id();
    let shown_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    Span::register_source_path(source_id, shown_path.display().to_string());
    let tokens = crate::lexer::tokenize_with_mode_and_source_id(&content, source_mode, source_id)
        .map_err(|e| AutoloadFileError::Unparsed(e.with_file(file_label.clone())))?;
    let parsed = crate::parser::parse_with_mode(&tokens, source_mode)
        .map_err(|e| AutoloadFileError::Unparsed(e.with_file(file_label.clone())))?;
    let parsed =
        crate::source::finalize_physical_program(parsed, &content, path, source_mode, defines)?;
    // `continuing`, not the root entry: the root one rewinds the source identities, which handed
    // this file's includes the identities of the entry program's includes.
    let (resolved, nested_includes, included_sources) =
        crate::resolver::resolve_collecting_includes_continuing(
            parsed,
            path.parent().unwrap_or(base_dir),
            defines,
        )?;
    let resolved = collect_aliases(resolved).map_err(|e| e.with_file(file_label.clone()))?;
    let canonicalized: Vec<Stmt> = crate::name_resolver::resolve(resolved)?;
    // A file-scope `if` the LANGUAGE PROFILE already decides picks which declarations this file
    // contributes, and php resolves it before any of them exist. Folding it here — on the file's
    // own statement list, while it still IS a file — is what lets the `return` in the taken
    // branch reach the boundary restoration below. See
    // [`fold_file_scope_profile_conditions`], whose doc names the one place this must not move to.
    let canonicalized = fold_file_scope_profile_conditions(canonicalized);
    // An autoloaded file is included in STATEMENT position, so its own top-level `return` ends
    // THAT FILE and never the program that included it. These statements are spliced straight
    // into the entry program, so the boundary has to be restored here or the file's `return`
    // becomes the PROGRAM's return. See [`discard_autoloaded_file_return`].
    let canonicalized = discard_autoloaded_file_return(canonicalized);
    let mut declaration_sources = declaration_source_files(&canonicalized, &file_label);
    // Everything this file's own includes declared is attributed to the file that WROTE it, which
    // the walk above cannot know: by now those statements have been spliced in and are
    // indistinguishable from this file's. Per-file attribution therefore overwrites it.
    declaration_sources.class_likes.extend(included_sources.class_likes);
    declaration_sources.functions.extend(included_sources.functions);
    declaration_sources
        .unbound_conditional_declaration_sources
        .extend(included_sources.unbound_conditional_declaration_sources);
    declaration_sources
        .bound_conditional_declaration_names
        .extend(included_sources.bound_conditional_declaration_names);
    for unit in included_sources.source_units.into_values() {
        unit.insert_into(&mut declaration_sources.source_units, Span::dummy())?;
    }
    crate::resolver::SourceUnit {
        canonical_path: path.canonicalize().unwrap_or_else(|_| path.to_path_buf()),
        mode: source_mode,
        source: content.into(),
    }.insert_into(&mut declaration_sources.source_units, Span::dummy())?;
    // name_resolver has already flattened namespace nodes and canonicalized
    // declarations, so we splice the statements directly into the top-level
    // program.
    let canonicalized = activate_interface_declarations(canonicalized, path, &|_| true);
    Ok((canonicalized, nested_includes, declaration_sources))
}

/// Restores PHP's include boundary for a file the autoload pass splices into the program.
///
/// An autoloaded file is always included in STATEMENT position — Composer's generated
/// `autoload_real.php` does `require $file;` for every `autoload.files` entry, and its class
/// loader's `includeFile()` does the same for a class file. PHP's rule for that shape is that a
/// top-level `return` ends the INCLUDED FILE and hands its value back to the (discarded) include
/// expression; it does not return from the program that included it. `crate::resolver`'s
/// `discard_statement_include_return` already restores exactly this for a written-out
/// `require`/`include`, but the autoload pass does not travel that path: it parses the file on
/// its own and splices the name-resolved statements straight into the entry program's top level,
/// where a `Return` node means "return from the program".
///
/// MEASURED, and the reason this exists: with Composer's eager `autoload.files` in the closed
/// world, `vendor/symfony/polyfill-mbstring/bootstrap.php` contributes its last line —
/// `return require __DIR__.'/bootstrap72.php';` — as top-level statement 160 of a 1,848-statement
/// Symfony `--web` program. Everything after it, 91% of the program including the web prelude's
/// whole error-handling surface, sat after a statement that terminates. `optimize::propagate`
/// then truncated the top level there and the build died in the BACKEND with
/// `call to unknown function error_log`, because the checker's function table had kept what the
/// AST had lost. Nothing about that is Symfony-specific: any `autoload.files` entry ending in
/// `return` does it.
///
/// DECLARATIONS AFTER THE `return` ARE KEPT, because php keeps them. An unconditional top-level
/// `function`/`class` in an included file is bound when the file is COMPILED, not when the
/// statement is reached, so
///
/// ```php
/// <?php return 1; function after_return_fn() {}
/// ```
///
/// leaves `function_exists('after_return_fn')` TRUE in the includer (verified on php 8.5.10).
/// Only the unreachable EXECUTABLE tail is dropped.
///
/// SCANNING THE TOP LEVEL ONLY IS COMPLETE HERE, and that is a property of the call site rather
/// than an assumption: `load_autoloaded_file` runs this AFTER `name_resolver::resolve`, which
/// flattens every `NamespaceBlock` into its parent list (`name_resolver::statements::list`) —
/// including the wrappers the resolver puts around this file's OWN includes, whose file-scope
/// returns `crate::resolver`'s `discard_statement_include_return` has already handled.
fn discard_autoloaded_file_return(mut body: Program) -> Program {
    let Some((index, guarded)) = body.iter().enumerate().find_map(|(index, stmt)| {
        match &stmt.kind {
            StmtKind::Return(_) => Some((index, false)),
            _ if is_bare_if_ending_in_return(&stmt.kind) => Some((index, true)),
            _ => None,
        }
    }) else {
        return body;
    };
    if guarded {
        return end_file_under_guard(body, index);
    }
    let span = body[index].span;
    let tail = body.split_off(index + 1);
    // The returned EXPRESSION still runs — `return require 'x.php';` performs the include — so it
    // is kept as a statement whose value is discarded, which is what the include site does with it.
    let returned = std::mem::replace(
        &mut body[index],
        Stmt::new(StmtKind::Synthetic(Vec::new()), span),
    );
    if let StmtKind::Return(Some(value)) = returned.kind {
        body[index] = Stmt::new(StmtKind::ExprStmt(value), span);
    }
    // A class-like the file already declared before the `return` is not one php hoists again:
    // nikic/php-parser's `Internal/TokenPolyfill.php` declares `TokenPolyfill extends \PhpToken`
    // and returns on php 8, and its polyfill `class TokenPolyfill` after the `return` is never
    // declared there (keeping it declared the class twice).
    let declared_before = collect_declared_fqns(&body)
        .into_iter()
        .map(|name| crate::names::php_symbol_key(&name))
        .collect::<HashSet<_>>();
    body.extend(tail.into_iter().filter(|stmt| {
        is_file_scope_hoisted_declaration(&stmt.kind)
            && !declared_class_like_name(stmt)
                .is_some_and(|name| declared_before.contains(&crate::names::php_symbol_key(name)))
    }));
    body
}

/// The name a class-like declaration statement declares.
fn declared_class_like_name(stmt: &Stmt) -> Option<&str> {
    match &stmt.kind {
        StmtKind::ClassDecl { name, .. }
        | StmtKind::InterfaceDecl { name, .. }
        | StmtKind::TraitDecl { name, .. }
        | StmtKind::EnumDecl { name, .. } => Some(name),
        _ => None,
    }
}

/// Returns whether this file-scope statement is `if (C) { …; return; }` with nothing else in the
/// chain — a GUARDED END OF FILE, and the second shape a file-scope `return` is written in.
///
/// The chain has to be bare (no `elseif`, no `else`) for the rewrite in
/// [`end_file_under_guard`] to be a rewrite rather than a duplication: with one arm the
/// statements after the `if` run exactly when the condition is false, which an `else` says
/// directly. With several arms they run when SEVERAL conditions were false, and saying that
/// without a flag variable means copying them into every arm.
fn is_bare_if_ending_in_return(kind: &StmtKind) -> bool {
    let StmtKind::If {
        then_body,
        elseif_clauses,
        else_body,
        ..
    } = kind
    else {
        return false;
    };
    elseif_clauses.is_empty()
        && else_body.is_none()
        && matches!(then_body.last().map(|stmt| &stmt.kind), Some(StmtKind::Return(_)))
}

/// Restores php's include boundary for `if (C) { …; return; }` followed by more of the file.
///
/// php's rule is the same one [`discard_autoloaded_file_return`] documents — the `return` ends
/// THE FILE — but the statement is not at the top level, so the top-level scan walked past it and
/// the `return` was left to return from the PROGRAM. MEASURED: `symfony/polyfill-mbstring`'s
/// `bootstrap80.php` ends with `if (extension_loaded('mbstring')) { return; }`, and
/// `extension_loaded('mbstring')` is TRUE in a compiled binary, so the second of twelve eager
/// Composer `files` entries returned from the request program. Every route answered `200` with a
/// zero-byte body and no diagnostic of any kind.
///
/// The rewrite is the identity php already gives us:
///
/// ```php
/// if (C) { A; return; }   REST        ===        if (C) { A; } else { REST }
/// ```
///
/// REST runs exactly when the file did not end, which is what `else` says. DECLARATIONS IN REST
/// STAY AT FILE SCOPE rather than moving into the `else`, for the same reason the top-level path
/// keeps them: php binds an unconditional file-scope declaration when the file is COMPILED, so it
/// is bound whether or not the branch ran.
///
/// REST IS PROCESSED BEFORE IT IS NESTED, so a second guarded end-of-file inside it is rewritten
/// while it is still file scope. That ordering is the whole reason this recurses.
fn end_file_under_guard(mut body: Program, index: usize) -> Program {
    let rest = discard_autoloaded_file_return(body.split_off(index + 1));
    let (declarations, executable): (Program, Program) = rest
        .into_iter()
        .partition(|stmt| is_file_scope_hoisted_declaration(&stmt.kind));
    let span = body[index].span;
    let StmtKind::If {
        condition,
        mut then_body,
        ..
    } = std::mem::replace(&mut body[index].kind, StmtKind::Synthetic(Vec::new()))
    else {
        unreachable!("end_file_under_guard is only reached for a bare `if`");
    };
    // The returned EXPRESSION still runs, exactly as on the top-level path.
    if let Some(returned) = then_body.pop() {
        let return_span = returned.span;
        if let StmtKind::Return(Some(value)) = returned.kind {
            then_body.push(Stmt::new(StmtKind::ExprStmt(value), return_span));
        }
    }
    body[index] = Stmt::new(
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses: Vec::new(),
            else_body: (!executable.is_empty()).then_some(executable),
        },
        span,
    );
    body.extend(declarations);
    body
}

/// Resolves a file-scope `if` whose condition the SELECTED LANGUAGE PROFILE already decides,
/// replacing it with the statements of the branch php would take.
///
/// WHY A FILE NEEDS THIS AT ALL. A library that has to work on several PHP versions ships one
/// entry file that picks the implementation for the running one, and the idiom is a file-scope
/// `return` under a version test:
///
/// ```php
/// <?php
/// if (\PHP_VERSION_ID >= 80000) {
///     return require __DIR__.'/bootstrap80.php';
/// }
/// if (!function_exists('thing')) { function thing() { /* pre-8.0 fallback */ } }
/// ```
///
/// php evaluates that condition BEFORE either declaration exists and contributes exactly one of
/// the two bodies. elephc's profile is fixed at compile time, so the condition is a constant here
/// too — but the `return` sits inside the `if`, where [`discard_autoloaded_file_return`]'s
/// top-level scan cannot see it, so the whole file was contributed: the fallback declaration AND
/// the required file's. MEASURED on the Symfony `--web` build: `symfony/polyfill-php85` and
/// `symfony/polyfill-intl-grapheme` each contributed a `grapheme_levenshtein` php never declares,
/// and the assembler refused the program with `symbol '_fn_grapheme_u_levenshtein' is already
/// defined`. Worse and quieter, the `return` in the taken branch stayed EXECUTABLE at the
/// program's top level, so the spliced file returned from the PROGRAM: a compiled entry that
/// eagerly loads such a file printed nothing at all.
///
/// THIS MUST RUN HERE, ON ONE FILE, AND NOT IN `optimize::fold`. By the time the optimizer sees
/// the program every autoloaded file has been spliced into one flat top level, and folding this
/// `if` there turns the file's `return` into the PROGRAM's: `optimize::propagate` stops at the
/// first statement that does not fall through and deletes everything after it. That is the
/// 1,688-of-1,848-statement deletion [`discard_autoloaded_file_return`] documents, and it
/// surfaced four passes away as `call to unknown function error_log`. Folding while the
/// statements are still A FILE is what makes the difference: the `return` is then this file's,
/// and the boundary restoration immediately below degrades it.
///
/// WHAT COUNTS AS DECIDED is deliberately narrow — see [`profile_condition`]. Only the
/// `PHP_*_VERSION*` family is folded, because those are the constants whose values the compiler
/// FIXES for the whole build and no program can change at runtime. `extension_loaded()`,
/// `function_exists()` and `defined()` are left alone here even where the closed world could
/// answer them: each is a call, and a fold that is wrong about one silently deletes a
/// declaration.
///
/// TOP LEVEL ONLY. A `return` at any deeper nesting is inside a function, a loop or a `try`,
/// where it means what it says; only a file-scope one ends the file. The taken branch's own
/// statements BECOME file scope, so they are folded in turn.
fn fold_file_scope_profile_conditions(body: Program) -> Program {
    let mut folded: Program = Vec::with_capacity(body.len());
    for stmt in body {
        let StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } = stmt.kind
        else {
            folded.push(stmt);
            continue;
        };
        match decided_branch(
            condition,
            then_body,
            elseif_clauses,
            else_body,
            stmt.span,
            &profile_condition,
        ) {
            Ok(taken) => folded.extend(fold_file_scope_profile_conditions(taken)),
            Err(untouched) => folded.push(untouched),
        }
    }
    folded
}

/// Folds a file-scope `if` that chooses WHICH class-like a class file declares by an existence
/// probe the closed world answers: `if (!class_exists(StaticReflectionService::class)) { trait T
/// {...} } else { trait T {...} }` (Doctrine's `GetReflectionClassImplementation`), or
/// `if (interface_exists(SimpleCacheInterface::class)) { class E ... } else { class E ... }`
/// (Symfony Cache's exception classes).
///
/// php runs that `if` when the autoloader includes the file, and with the probe autoloading
/// (`class_exists(X::class)`, or an explicit `true`), its answer is whether X can be loaded --
/// which the compile-time autoload registry is the authority on, exactly as it already is for
/// [`unsatisfiable_file_scope_guard`]. Left undecided, the file declared its class-like nowhere
/// the compiler could take it, so the whole file went to the interpreter, and every class using
/// the trait or extending the exception with it (`ClassMetadata`, the Cache adapters).
///
/// Narrow on purpose: only an `if` whose branches DECLARE a class-like is folded (anything else
/// keeps running at run time, as [`fold_file_scope_profile_conditions`] explains), and only a
/// probe of a literal name that autoloads.
fn fold_file_scope_existence_conditions(
    body: Program,
    declared: &HashSet<String>,
    registry: &Registry,
    base_dir: &Path,
    defines: &HashSet<String>,
) -> Program {
    let available = |name: &str| {
        let key = crate::names::php_symbol_key(name);
        declared
            .iter()
            .any(|declared| crate::names::php_symbol_key(declared) == key)
            || resolve_class(name, registry).is_some()
    };
    let has_method = |class: &str, method: &str| {
        autoloaded_class_has_method(class, method, registry, base_dir, defines, 0)
    };
    let kind_ok = |function: &str, class: &str| {
        let kind = autoloaded_declaration_kind(class, registry, base_dir, defines)?;
        Some(match function {
            "class_exists" => matches!(kind, DeclarationKind::Class | DeclarationKind::Enum),
            "interface_exists" => kind == DeclarationKind::Interface,
            "trait_exists" => kind == DeclarationKind::Trait,
            "enum_exists" => kind == DeclarationKind::Enum,
            _ => return None,
        })
    };
    let decide =
        |condition: &Expr| existence_condition(condition, &available, &has_method, &kind_ok);
    let has_return_type = |class: &str, method: &str, probe: TypeProbe| {
        autoloaded_method_has_return_type(class, method, probe, registry, base_dir, defines, 0)
    };
    let mut folded = HashSet::new();
    let mut probes = ProbeResults::default();
    let body = fold_reflection_require_ternaries(
        body,
        &decide,
        &has_return_type,
        &mut HashMap::new(),
        &mut probes,
        &mut folded,
    );
    let body = drop_unread_probe_results(body, &probes.assigned);
    let body = drop_consumed_reflection_bindings(body, &folded);
    // After the fold: a profile-decided `require PHP_VERSION < 8.2 ? A : B` only exposes the
    // self-guarded declaration of the file it keeps once its branch is chosen.
    let body = fold_existence_conditions_in(body, &decide);
    inline_self_guarded_declarations(body, &available)
}

/// Picks the branch of a file-scope `require $m->hasReturnType() ? A : B;` whose `$m` is a
/// `new ReflectionMethod(<class>, '<method>')` bound earlier in the same file.
///
/// Carbon's `Translator.php` chooses its lazy `LazyTranslator` this way, after
/// `$m = new ReflectionMethod(class_exists(TranslatorInterface::class) ? TranslatorInterface::class
/// : Translation\Translator::class, 'trans')`. php answers it while including the file, from the
/// installed sources, which the autoload registry resolves: left to run time, the file declared
/// `Translator` nowhere the compiler could take it. Anything undecided keeps the `require`.
///
/// Each binding a fold consumed is recorded in `folded`, for [`drop_consumed_reflection_bindings`].
///
/// psysh's `VarDumper/Dumper.php` stores the probes first and declares a different trait in each
/// arm of `if ($typed) { if ($returnTyped) … } elseif ($returnTyped) … else …`: a variable holding
/// a decided probe decides the conditions that read it, `elseif` chains fold arm by arm, and the
/// kept arm is folded in turn. The probe assignments nothing reads any more are dropped
/// ([`drop_unread_probe_results`]).
fn fold_reflection_require_ternaries(
    body: Program,
    decide: &dyn Fn(&Expr) -> Option<bool>,
    has_return_type: &dyn Fn(&str, &str, TypeProbe) -> Option<bool>,
    bindings: &mut HashMap<String, (Expr, String)>,
    probes: &mut ProbeResults,
    folded: &mut HashSet<String>,
) -> Program {
    let mut out: Program = Vec::with_capacity(body.len());
    for mut stmt in body {
        match &mut stmt.kind {
            StmtKind::NamespaceBlock { body, .. } => {
                let inner = std::mem::take(body);
                *body = fold_reflection_require_ternaries(
                    inner,
                    decide,
                    has_return_type,
                    bindings,
                    probes,
                    folded,
                );
            }
            StmtKind::Assign { name, value } => {
                probes.values.remove(name.as_str());
                if let Some(binding) = reflection_method_binding(value) {
                    bindings.insert(name.clone(), binding);
                } else {
                    bindings.remove(name.as_str());
                    if let Some(answer) =
                        decided_probe(value, bindings, decide, has_return_type, folded)
                    {
                        probes.values.insert(name.clone(), answer);
                        probes.assigned.insert(name.clone());
                    }
                }
            }
            StmtKind::Include { path, .. } => {
                if let ExprKind::Ternary { condition, then_expr, else_expr } = &path.kind {
                    let chosen =
                        decided_probe(condition, bindings, decide, has_return_type, folded);
                    if let Some(chosen) = chosen {
                        let branch = if chosen { then_expr } else { else_expr };
                        *path = (**branch).clone();
                    }
                }
            }
            // The resolver has usually rewritten the static ternary already, into
            // `if ($m->hasReturnType()) { require A; } else { require B; }` with both files spliced.
            StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
                let mut arms = std::iter::once((&*condition, &mut *then_body))
                    .chain(elseif_clauses.iter_mut().map(|(c, b)| (&*c, b)));
                let mut chosen: Option<Program> = None;
                let mut undecided = false;
                for (arm_condition, arm_body) in &mut arms {
                    match decided_probe(arm_condition, bindings, decide, has_return_type, folded)
                        .or_else(|| match &arm_condition.kind {
                            ExprKind::Variable(name) => probes.values.get(name.as_str()).copied(),
                            _ => None,
                        }) {
                        Some(true) => {
                            chosen = Some(std::mem::take(arm_body));
                            break;
                        }
                        Some(false) => continue,
                        None => {
                            undecided = true;
                            break;
                        }
                    }
                }
                drop(arms);
                if !undecided {
                    let branch = chosen.unwrap_or_else(|| else_body.take().unwrap_or_default());
                    let branch = fold_reflection_require_ternaries(
                        branch,
                        decide,
                        has_return_type,
                        bindings,
                        probes,
                        folded,
                    );
                    out.push(Stmt::new(StmtKind::Synthetic(branch), stmt.span));
                    continue;
                }
            }
            _ => {}
        }
        out.push(stmt);
    }
    out
}

/// The probe results [`fold_reflection_require_ternaries`] stored in variables.
#[derive(Default)]
struct ProbeResults {
    /// The answer each variable holds now.
    values: HashMap<String, bool>,
    /// Every variable some decided probe was assigned to.
    assigned: HashSet<String>,
}

/// Answers a `$m->hasReturnType()` / `$m->getParameters()[i]->hasType()` probe on a bound
/// `ReflectionMethod`, recording the binding it consumed.
fn decided_probe(
    probe: &Expr,
    bindings: &HashMap<String, (Expr, String)>,
    decide: &dyn Fn(&Expr) -> Option<bool>,
    has_return_type: &dyn Fn(&str, &str, TypeProbe) -> Option<bool>,
    folded: &mut HashSet<String>,
) -> Option<bool> {
    let (variable, kind) = has_return_type_probe(probe)?;
    let (class, method) = bindings.get(variable)?;
    let class = decided_class_name(class, decide)?;
    let answer = has_return_type(&class, method, kind)?;
    folded.insert(variable.to_string());
    Some(answer)
}

/// Drops `$typed = $m->...->hasType()` assignments of decided probes the file no longer reads,
/// so the `ReflectionMethod` bindings they read become unread in turn.
fn drop_unread_probe_results(body: Program, assigned: &HashSet<String>) -> Program {
    if assigned.is_empty() {
        return body;
    }
    let mut read = HashSet::new();
    crate::parser::ast::visit::walk_nodes(&body, &mut |node| {
        if let crate::parser::ast::visit::Node::Expr(expr) = node {
            if let ExprKind::Variable(name) = &expr.kind {
                read.insert(name.clone());
            }
        }
    });
    let unread: HashSet<String> = assigned.difference(&read).cloned().collect();
    fn drop_named(body: Program, unread: &HashSet<String>) -> Program {
        body.into_iter()
            .filter_map(|mut stmt| {
                match &mut stmt.kind {
                    StmtKind::NamespaceBlock { body, .. } => {
                        let inner = std::mem::take(body);
                        *body = drop_named(inner, unread);
                    }
                    StmtKind::Assign { name, value }
                        if unread.contains(name.as_str()) && has_return_type_probe(value).is_some() =>
                    {
                        return None;
                    }
                    _ => {}
                }
                Some(stmt)
            })
            .collect()
    }
    drop_named(body, &unread)
}

/// Removes the `$m = new ReflectionMethod(...)` bindings whose only use a fold consumed.
///
/// php runs the constructor while including the file, but Composer includes a class file from
/// inside a function, so `$m` is a local of the autoloader nobody can observe. Left in place it
/// is file-scope code with an effect, and `Translator.php` was deferred for it after the
/// `require` itself had been decided. A binding the file reads again is kept.
fn drop_consumed_reflection_bindings(body: Program, folded: &HashSet<String>) -> Program {
    if folded.is_empty() {
        return body;
    }
    let reads = |name: &str| {
        let mut count = 0usize;
        crate::parser::ast::visit::walk_nodes(&body, &mut |node| {
            if let crate::parser::ast::visit::Node::Expr(expr) = node {
                count += usize::from(matches!(&expr.kind, ExprKind::Variable(v) if v == name));
            }
        });
        count
    };
    let unread: HashSet<String> = folded.iter().filter(|name| reads(name) == 0).cloned().collect();
    drop_reflection_bindings_named(body, &unread)
}

/// Drops `$name = new ReflectionMethod(...)` statements for every `name` in `names`.
fn drop_reflection_bindings_named(body: Program, names: &HashSet<String>) -> Program {
    body.into_iter()
        .filter_map(|mut stmt| {
            match &mut stmt.kind {
                StmtKind::NamespaceBlock { body, .. } => {
                    let inner = std::mem::take(body);
                    *body = drop_reflection_bindings_named(inner, names);
                }
                StmtKind::Assign { name, value }
                    if names.contains(name.as_str()) && reflection_method_binding(value).is_some() =>
                {
                    return None;
                }
                _ => {}
            }
            Some(stmt)
        })
        .collect()
}

/// `(class expression, method)` for `new ReflectionMethod(<class>, '<method>')`.
fn reflection_method_binding(value: &Expr) -> Option<(Expr, String)> {
    let ExprKind::NewObject { class_name, args } = &value.kind else {
        return None;
    };
    if !class_name
        .as_canonical()
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("ReflectionMethod")
        || args.len() != 2
    {
        return None;
    }
    let ExprKind::StringLiteral(method) = &args[1].kind else {
        return None;
    };
    Some((args[0].clone(), method.clone()))
}

/// The variable of a `$m->hasReturnType()` condition.
fn has_return_type_probe(condition: &Expr) -> Option<(&str, TypeProbe)> {
    let ExprKind::MethodCall { object, method, args } = &condition.kind else {
        return None;
    };
    if !args.is_empty() {
        return None;
    }
    match &object.kind {
        ExprKind::Variable(variable) if method.eq_ignore_ascii_case("hasReturnType") => {
            Some((variable.as_str(), TypeProbe::Return))
        }
        // Carbon's `MessageFormatterMapper.php`:
        // `require $m->getParameters()[0]->hasType() ? <strong> : <weak>;`.
        ExprKind::ArrayAccess { array, index } if method.eq_ignore_ascii_case("hasType") => {
            let ExprKind::IntLiteral(position) = index.kind else {
                return None;
            };
            let ExprKind::MethodCall { object, method, args } = &array.kind else {
                return None;
            };
            let ExprKind::Variable(variable) = &object.kind else {
                return None;
            };
            (method.eq_ignore_ascii_case("getParameters") && args.is_empty() && position >= 0)
                .then_some((variable.as_str(), TypeProbe::Param(position as usize)))
        }
        _ => None,
    }
}

/// Which declared type a folded reflection probe asks about.
#[derive(Clone, Copy)]
enum TypeProbe {
    /// `hasReturnType()`.
    Return,
    /// `getParameters()[i]->hasType()`.
    Param(usize),
}

/// A class named by a literal, `X::class`, or a `cond ? X::class : Y::class` the probes decide.
fn decided_class_name(expr: &Expr, decide: &dyn Fn(&Expr) -> Option<bool>) -> Option<String> {
    match &expr.kind {
        ExprKind::StringLiteral(name) => Some(name.trim_start_matches('\\').to_string()),
        ExprKind::ClassConstant { receiver: crate::parser::ast::StaticReceiver::Named(name) } => {
            Some(name.as_canonical().trim_start_matches('\\').to_string())
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            let branch = if decide(condition)? { then_expr } else { else_expr };
            decided_class_name(branch, decide)
        }
        _ => None,
    }
}

/// Answers `(new ReflectionMethod(X, 'm'))->hasReturnType()` from X's autoloaded source: the
/// first declaration of `m` found in X, then its parents, interfaces and traits. `None` when a
/// class-like on the way cannot be read, or nothing declares `m`.
fn autoloaded_method_has_return_type(
    class: &str,
    method: &str,
    probe: TypeProbe,
    registry: &Registry,
    base_dir: &Path,
    defines: &HashSet<String>,
    depth: usize,
) -> Option<bool> {
    if depth > 32 {
        return None;
    }
    let path = resolve_class(class, registry)?;
    let (program, _, _) = load_autoloaded_file(&path, base_dir, defines).ok()?;
    let key = crate::names::php_symbol_key(class);
    let mut found: Option<(Option<bool>, Vec<String>)> = None;
    crate::parser::ast::visit::walk_stmts(&program, &mut |stmt| {
        if found.is_some() {
            return;
        }
        let (name, methods, ancestors): (&String, &Vec<_>, Vec<String>) = match &stmt.kind {
            StmtKind::ClassDecl { name, extends, implements, trait_uses, methods, .. } => (
                name,
                methods,
                extends
                    .iter()
                    .chain(implements)
                    .chain(trait_uses.iter().flat_map(|uses| &uses.trait_names))
                    .map(|n| n.as_canonical().trim_start_matches('\\').to_string())
                    .collect(),
            ),
            StmtKind::InterfaceDecl { name, extends, methods, .. } => (
                name,
                methods,
                extends.iter().map(|n| n.as_canonical().trim_start_matches('\\').to_string()).collect(),
            ),
            StmtKind::TraitDecl { name, trait_uses, methods, .. } => (
                name,
                methods,
                trait_uses
                    .iter()
                    .flat_map(|uses| &uses.trait_names)
                    .map(|n| n.as_canonical().trim_start_matches('\\').to_string())
                    .collect(),
            ),
            _ => return,
        };
        if crate::names::php_symbol_key(name.trim_start_matches('\\')) != key {
            return;
        }
        let own = methods
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(method))
            .and_then(|m| match probe {
                TypeProbe::Return => Some(m.return_type.is_some()),
                // A parameter the method lacks is php's `Undefined array key`, not a fold.
                TypeProbe::Param(position) => m.params.get(position).map(|param| param.1.is_some()),
            });
        found = Some((own, ancestors));
    });
    let (own, ancestors) = found?;
    if own.is_some() {
        return own;
    }
    ancestors.iter().find_map(|ancestor| {
        autoloaded_method_has_return_type(ancestor, method, probe, registry, base_dir, defines, depth + 1)
    })
}

/// The kind of class-like declaration an autoloaded `class` names (`None`: unreadable or absent).
#[derive(Clone, Copy, PartialEq, Eq)]
enum DeclarationKind {
    Class,
    Interface,
    Trait,
    Enum,
}

fn autoloaded_declaration_kind(
    class: &str,
    registry: &Registry,
    base_dir: &Path,
    defines: &HashSet<String>,
) -> Option<DeclarationKind> {
    let path = resolve_class(class, registry)?;
    let (program, _, _) = load_autoloaded_file(&path, base_dir, defines).ok()?;
    let key = crate::names::php_symbol_key(class.trim_start_matches('\\'));
    let mut found = None;
    crate::parser::ast::visit::walk_stmts(&program, &mut |stmt| {
        if found.is_some() {
            return;
        }
        let (name, kind) = match &stmt.kind {
            StmtKind::ClassDecl { name, .. } => (name, DeclarationKind::Class),
            StmtKind::InterfaceDecl { name, .. } => (name, DeclarationKind::Interface),
            StmtKind::TraitDecl { name, .. } => (name, DeclarationKind::Trait),
            StmtKind::EnumDecl { name, .. } => (name, DeclarationKind::Enum),
            _ => return,
        };
        if crate::names::php_symbol_key(name.trim_start_matches('\\')) == key {
            found = Some(kind);
        }
    });
    found
}

/// Inlines `if (!class_exists(X::class, false)) { class X ... }` when nothing else can supply X.
///
/// The non-autoloading probe is otherwise left to run time because its answer depends on load
/// order. A guard around the file's OWN declaration of X, for an X no other declaration or
/// autoload entry provides, has one answer the first time the file runs: X is absent, the branch
/// declares it. Carbon's lazy `DatePeriodBase`/`LazyTranslator` files have exactly this shape,
/// and refusing it sent `CarbonPeriod` and `Translator` (and with them every date in Laravel) to
/// the interpreter.
fn inline_self_guarded_declarations(body: Program, available: &dyn Fn(&str) -> bool) -> Program {
    let mut out: Program = Vec::with_capacity(body.len());
    for stmt in body {
        let span = stmt.span;
        match stmt.kind {
            StmtKind::NamespaceBlock { name, body } => {
                let mut rebuilt = Stmt::new(
                    StmtKind::NamespaceBlock {
                        name,
                        body: inline_self_guarded_declarations(body, available),
                    },
                    span,
                );
                rebuilt.source_mode = stmt.source_mode;
                rebuilt.strict_types = stmt.strict_types;
                rebuilt.attributes = stmt.attributes;
                out.push(rebuilt);
            }
            // A required file arrives wrapped (`Synthetic`, `IncludeOnceGuard`), and its guard is
            // still file scope inside the wrapper.
            StmtKind::Synthetic(body) => {
                let mut rebuilt = Stmt::new(
                    StmtKind::Synthetic(inline_self_guarded_declarations(body, available)),
                    span,
                );
                rebuilt.source_mode = stmt.source_mode;
                rebuilt.strict_types = stmt.strict_types;
                rebuilt.attributes = stmt.attributes;
                out.push(rebuilt);
            }
            StmtKind::IncludeOnceGuard { source_path, required, body } => {
                let mut rebuilt = Stmt::new(
                    StmtKind::IncludeOnceGuard {
                        source_path,
                        required,
                        body: inline_self_guarded_declarations(body, available),
                    },
                    span,
                );
                rebuilt.source_mode = stmt.source_mode;
                rebuilt.strict_types = stmt.strict_types;
                rebuilt.attributes = stmt.attributes;
                out.push(rebuilt);
            }
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } if elseif_clauses.is_empty()
                && else_body.is_none()
                && self_guarded_class(&condition, &then_body)
                    .is_some_and(|class| !available(&class)) =>
            {
                out.extend(then_body);
            }
            kind => {
                let mut rebuilt = Stmt::new(kind, span);
                rebuilt.source_mode = stmt.source_mode;
                rebuilt.strict_types = stmt.strict_types;
                rebuilt.attributes = stmt.attributes;
                out.push(rebuilt);
            }
        }
    }
    out
}

/// The class a `!class_exists(X::class, false)`-style guard probes, when its body declares X.
fn self_guarded_class(condition: &Expr, then_body: &[Stmt]) -> Option<String> {
    let ExprKind::Not(inner) = &condition.kind else {
        return None;
    };
    let ExprKind::FunctionCall { name, args } = &inner.kind else {
        return None;
    };
    let function = name.as_canonical().trim_start_matches('\\').to_ascii_lowercase();
    if !matches!(
        function.as_str(),
        "class_exists" | "interface_exists" | "trait_exists" | "enum_exists"
    ) {
        return None;
    }
    let class = match &args.first()?.kind {
        ExprKind::ClassConstant {
            receiver: crate::parser::ast::StaticReceiver::Named(name),
        } => name.as_canonical().trim_start_matches('\\').to_string(),
        ExprKind::StringLiteral(name) => name.trim_start_matches('\\').to_string(),
        _ => return None,
    };
    let key = crate::names::php_symbol_key(&class);
    let declares = then_body.iter().any(|stmt| match &stmt.kind {
        StmtKind::ClassDecl { name, .. }
        | StmtKind::InterfaceDecl { name, .. }
        | StmtKind::TraitDecl { name, .. }
        | StmtKind::EnumDecl { name, .. } => {
            crate::names::php_symbol_key(name.trim_start_matches('\\')) == key
        }
        _ => false,
    });
    declares.then_some(class)
}

/// Applies [`fold_file_scope_existence_conditions`] to one statement list, through namespaces.
fn fold_existence_conditions_in(body: Program, decide: &dyn Fn(&Expr) -> Option<bool>) -> Program {
    let mut folded: Program = Vec::with_capacity(body.len());
    for stmt in body {
        let span = stmt.span;
        // A rebuilt statement keeps the metadata of the one it replaces. `Stmt::new` takes the
        // source profile and `strict_types` from the CURRENT parse (the entry file's, not the
        // autoloaded file's) and starts with no attributes -- and a declaration's PHP attributes
        // (`#[AsCommand(...)]` on a class) live on the statement. Every class-like an autoloaded
        // file declares lost them: `ReflectionClass::getAttributes()` answered nothing for all of
        // Composer's classes, so every Symfony console command lost its name.
        let source_mode = stmt.source_mode;
        let strict_types = stmt.strict_types;
        let attributes = stmt.attributes;
        let rebuild = move |kind: StmtKind| {
            let mut rebuilt = Stmt::new(kind, span);
            rebuilt.source_mode = source_mode;
            rebuilt.strict_types = strict_types;
            rebuilt.attributes = attributes.clone();
            rebuilt
        };
        match stmt.kind {
            StmtKind::NamespaceBlock { name, body } => folded.push(rebuild(StmtKind::NamespaceBlock {
                name,
                body: fold_existence_conditions_in(body, decide),
            })),
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                let declares_class_like = walk::first_conditionally_declared_class_like(&vec![Stmt::new(
                    StmtKind::If {
                        condition: Expr::new(ExprKind::BoolLiteral(true), span),
                        then_body: then_body.clone(),
                        elseif_clauses: elseif_clauses.clone(),
                        else_body: else_body.clone(),
                    },
                    span,
                )])
                .is_some();
                // A throwing guard over a class the compiled world provides
                // (`if (!interface_exists(LocaleAwareInterface::class)) { throw ...; }` atop
                // symfony/string's `AsciiSlugger.php`) never throws: php's probe autoloads the
                // class. Kept, it was spliced among the boot-time declarations and ran before
                // anything registered an autoloader and, in the demo console, before the
                // interface's own activation -- every command died on "the
                // symfony/translation-contracts package is not installed".
                if !declares_class_like
                    && elseif_clauses.is_empty()
                    && else_body.is_none()
                    && then_body.first().is_some_and(stmt_throws)
                    && decide(&condition) == Some(false)
                {
                    continue;
                }
                if !declares_class_like {
                    folded.push(rebuild(StmtKind::If {
                        condition,
                        then_body,
                        elseif_clauses,
                        else_body,
                    }));
                    continue;
                }
                match decided_branch(condition, then_body, elseif_clauses, else_body, span, decide) {
                    Ok(taken) => folded.extend(fold_existence_conditions_in(taken, decide)),
                    Err(mut untouched) => {
                        untouched.source_mode = source_mode;
                        untouched.strict_types = strict_types;
                        folded.push(untouched);
                    }
                }
            }
            kind => folded.push(rebuild(kind)),
        }
    }
    folded
}

/// Evaluates an autoloading class-like existence probe (and `!`/`&&`/`||` over probes).
///
/// `has_method(class, method)` answers `method_exists(X::class, 'm')` (`None`: undecided).
fn existence_condition(
    expr: &Expr,
    available: &dyn Fn(&str) -> bool,
    has_method: &dyn Fn(&str, &str) -> Option<bool>,
    kind_ok: &dyn Fn(&str, &str) -> Option<bool>,
) -> Option<bool> {
    match &expr.kind {
        ExprKind::Not(inner) => {
            existence_condition(inner, available, has_method, kind_ok).map(|value| !value)
        }
        ExprKind::BinaryOp {
            left,
            op: BinOp::And,
            right,
        } => match existence_condition(left, available, has_method, kind_ok)? {
            false => Some(false),
            true => existence_condition(right, available, has_method, kind_ok),
        },
        ExprKind::BinaryOp {
            left,
            op: BinOp::Or,
            right,
        } => match existence_condition(left, available, has_method, kind_ok)? {
            true => Some(true),
            false => existence_condition(right, available, has_method, kind_ok),
        },
        ExprKind::FunctionCall { name: function, .. } => {
            if let Some((class, method)) = method_exists_probe(expr) {
                return has_method(&class, &method);
            }
            let negated = Expr::new(ExprKind::Not(Box::new(expr.clone())), expr.span);
            let name = negated_existence_probe(&negated)?;
            if !available(&name) {
                return Some(false);
            }
            // `class_exists()` is false for an interface or trait, `interface_exists()` for a
            // class: Carbon's `Translator.php` asks `class_exists(TranslatorInterface::class)`, an
            // interface, and php takes the other branch. The declaration's kind decides when
            // its file can be read; otherwise availability alone answers, as before.
            let function = function.as_canonical().trim_start_matches('\\').to_ascii_lowercase();
            Some(kind_ok(&function, &name).unwrap_or(true))
        }
        _ => profile_condition(expr),
    }
}

/// Returns `(X, m)` for `method_exists(X::class, 'm')` (or a literal class name).
fn method_exists_probe(expr: &Expr) -> Option<(String, String)> {
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return None;
    };
    if !name
        .as_canonical()
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("method_exists")
        || args.len() != 2
    {
        return None;
    }
    let class = match &args[0].kind {
        ExprKind::ClassConstant {
            receiver: crate::parser::ast::StaticReceiver::Named(name),
        } => name.as_canonical().trim_start_matches('\\').to_string(),
        ExprKind::StringLiteral(name) => name.trim_start_matches('\\').to_string(),
        _ => return None,
    };
    let ExprKind::StringLiteral(method) = &args[1].kind else {
        return None;
    };
    (!class.is_empty()).then(|| (class, method.clone()))
}

/// Answers `method_exists(X::class, 'm')` from X's autoloaded source: whether X, one of its
/// parents, interfaces or traits declares `m`. `None` when some class-like on the way cannot be
/// read from the autoload registry, which leaves the condition to run time.
///
/// Symfony's `TwigExtraBundle` declares itself twice under
/// `if (method_exists(KernelInterface::class, 'getShareDir'))`: php answers that while including
/// the file, from the installed `KernelInterface`, which is the file the registry resolves.
fn autoloaded_class_has_method(
    class: &str,
    method: &str,
    registry: &Registry,
    base_dir: &Path,
    defines: &HashSet<String>,
    depth: usize,
) -> Option<bool> {
    if depth > 32 {
        return None;
    }
    let path = resolve_class(class, registry)?;
    let (program, _, _) = load_autoloaded_file(&path, base_dir, defines).ok()?;
    let key = crate::names::php_symbol_key(class);
    let declaration = program.iter().find_map(|stmt| {
        let (name, methods, ancestors): (&String, &Vec<_>, Vec<&crate::names::Name>) = match &stmt.kind {
            StmtKind::ClassDecl { name, extends, implements, trait_uses, methods, .. } => (
                name,
                methods,
                extends
                    .iter()
                    .chain(implements)
                    .chain(trait_uses.iter().flat_map(|uses| &uses.trait_names))
                    .collect(),
            ),
            StmtKind::InterfaceDecl { name, extends, methods, .. } => {
                (name, methods, extends.iter().collect())
            }
            StmtKind::TraitDecl { name, trait_uses, methods, .. } => (
                name,
                methods,
                trait_uses.iter().flat_map(|uses| &uses.trait_names).collect(),
            ),
            StmtKind::EnumDecl { name, implements, trait_uses, methods, .. } => (
                name,
                methods,
                implements
                    .iter()
                    .chain(trait_uses.iter().flat_map(|uses| &uses.trait_names))
                    .collect(),
            ),
            _ => return None,
        };
        (crate::names::php_symbol_key(name.trim_start_matches('\\')) == key)
            .then_some((methods, ancestors))
    })?;
    let (methods, ancestors) = declaration;
    if methods.iter().any(|m| m.name.eq_ignore_ascii_case(method)) {
        return Some(true);
    }
    for ancestor in ancestors {
        let ancestor = ancestor.as_canonical().trim_start_matches('\\').to_string();
        if autoloaded_class_has_method(&ancestor, method, registry, base_dir, defines, depth + 1)? {
            return Some(true);
        }
    }
    Some(false)
}

/// Picks the branch of an `if`/`elseif`/`else` chain the profile decides, or hands the statement
/// back unchanged.
///
/// Conditions are evaluated in source order and the FIRST decided-true one wins, which is php's
/// own rule. An undecidable condition stops the walk and returns the whole statement untouched:
/// a later arm cannot be chosen over a guard whose value is unknown, and an earlier arm was
/// already known false, so nothing is lost by leaving the chain to run.
fn decided_branch(
    condition: Expr,
    then_body: Program,
    elseif_clauses: Vec<(Expr, Program)>,
    else_body: Option<Program>,
    span: Span,
    decide: &dyn Fn(&Expr) -> Option<bool>,
) -> Result<Program, Stmt> {
    let rebuild = |condition, then_body, elseif_clauses, else_body| {
        Stmt::new(
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            },
            span,
        )
    };
    match decide(&condition) {
        Some(true) => return Ok(then_body),
        None => return Err(rebuild(condition, then_body, elseif_clauses, else_body)),
        Some(false) => {}
    }
    for (index, (guard, _)) in elseif_clauses.iter().enumerate() {
        match decide(guard) {
            Some(true) => {
                let mut clauses = elseif_clauses;
                return Ok(clauses.swap_remove(index).1);
            }
            None => return Err(rebuild(condition, then_body, elseif_clauses, else_body)),
            Some(false) => {}
        }
    }
    Ok(else_body.unwrap_or_default())
}

/// Evaluates a condition the compile-time PHP profile decides, or `None` when it does not.
///
/// `None` is the answer for everything this cannot prove, including every call and every
/// variable. Short-circuiting is honoured in the direction that cannot lose a side effect: `A &&
/// B` is decided only once `A` is, because `f() && false` still calls `f()`.
fn profile_condition(expr: &Expr) -> Option<bool> {
    match &expr.kind {
        ExprKind::BoolLiteral(value) => Some(*value),
        ExprKind::Not(inner) => profile_condition(inner).map(|value| !value),
        ExprKind::BinaryOp { left, op, right } => match op {
            BinOp::And => match profile_condition(left)? {
                false => Some(false),
                true => profile_condition(right),
            },
            BinOp::Or => match profile_condition(left)? {
                true => Some(true),
                false => profile_condition(right),
            },
            BinOp::Lt
            | BinOp::Gt
            | BinOp::LtEq
            | BinOp::GtEq
            | BinOp::Eq
            | BinOp::NotEq
            | BinOp::StrictEq
            | BinOp::StrictNotEq => {
                if let Some(ordering) = profile_version_string_ordering(left, right) {
                    return Some(match op {
                        BinOp::Lt => ordering.is_lt(),
                        BinOp::Gt => ordering.is_gt(),
                        BinOp::LtEq => ordering.is_le(),
                        BinOp::GtEq => ordering.is_ge(),
                        BinOp::Eq => ordering.is_eq(),
                        BinOp::NotEq => ordering.is_ne(),
                        // `===` against a number is false for a string constant whatever it holds.
                        _ => return None,
                    });
                }
                let left = profile_int(left)?;
                let right = profile_int(right)?;
                Some(match op {
                    BinOp::Lt => left < right,
                    BinOp::Gt => left > right,
                    BinOp::LtEq => left <= right,
                    BinOp::GtEq => left >= right,
                    BinOp::Eq | BinOp::StrictEq => left == right,
                    _ => left != right,
                })
            }
            _ => None,
        },
        _ => profile_int(expr).map(|value| value != 0),
    }
}

/// Orders `PHP_VERSION` against a number or string literal the way php 8 compares them.
///
/// Carbon picks its lazy `DatePeriod` base with `require PHP_VERSION < 8.2 ? A : B;`. A version
/// string such as "8.5.0" is not numeric, so php compares the literal's string form with it
/// byte-wise ("8.5.0" vs "8.2"); two numeric strings compare as numbers.
fn profile_version_string_ordering(left: &Expr, right: &Expr) -> Option<std::cmp::Ordering> {
    fn version(expr: &Expr) -> Option<String> {
        let ExprKind::ConstRef(name) = &expr.kind else {
            return None;
        };
        if name.parts.last()?.as_str() != "PHP_VERSION" {
            return None;
        }
        let profile = crate::codegen_support::compile_php_version();
        Some(profile.reported_version_string())
    }
    fn literal(expr: &Expr) -> Option<String> {
        match &expr.kind {
            ExprKind::FloatLiteral(value) => Some(format!("{}", value)),
            ExprKind::IntLiteral(value) => Some(value.to_string()),
            ExprKind::StringLiteral(value) => Some(value.clone()),
            _ => None,
        }
    }
    fn compare(a: &str, b: &str) -> std::cmp::Ordering {
        match (a.parse::<f64>(), b.parse::<f64>()) {
            (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
            _ => a.as_bytes().cmp(b.as_bytes()),
        }
    }
    if let (Some(v), Some(l)) = (version(left), literal(right)) {
        return Some(compare(&v, &l));
    }
    if let (Some(l), Some(v)) = (literal(left), version(right)) {
        return Some(compare(&l, &v));
    }
    None
}

/// Evaluates an integer expression the compile-time PHP profile decides.
fn profile_int(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::Negate(inner) => profile_int(inner)?.checked_neg(),
        ExprKind::ConstRef(name) => profile_constant(name),
        _ => None,
    }
}

/// Returns the value of a PHP constant the compiler fixes for the whole build.
///
/// The set is the version family and nothing else. Every other compile-time-known constant is
/// left undecided on purpose: this fold DELETES source, so its inputs are limited to the values
/// that cannot differ between this compilation and the program it produces.
///
/// A PHP global constant is case-sensitive and has no namespace, so `\PHP_VERSION_ID` and
/// `PHP_VERSION_ID` are the same constant — the last segment is what is compared, as
/// `crate::opcache_prelude`'s own constant detection already does.
fn profile_constant(name: &crate::names::Name) -> Option<i64> {
    let profile = crate::codegen_support::compile_php_version();
    match name.parts.last()?.as_str() {
        "PHP_VERSION_ID" => Some(i64::from(profile.reported_version_id())),
        "PHP_MAJOR_VERSION" => Some(i64::from(profile.major())),
        "PHP_MINOR_VERSION" => Some(i64::from(profile.minor())),
        "PHP_RELEASE_VERSION" => Some(i64::from(profile.reported_release())),
        _ => None,
    }
}

/// Returns whether php binds this declaration when the file is COMPILED rather than when the
/// statement is reached, which is what makes it survive a `return` earlier in the same file.
///
/// The list mirrors `crate::resolver`'s `is_discoverable_declaration`, which is the same question
/// asked at the other include site; the two must agree or one path keeps a declaration the other
/// drops.
fn is_file_scope_hoisted_declaration(kind: &StmtKind) -> bool {
    matches!(
        kind,
        StmtKind::FunctionDecl { .. }
            | StmtKind::ClassDecl { .. }
            | StmtKind::EnumDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. }
            | StmtKind::PackedClassDecl { .. }
            | StmtKind::ExternFunctionDecl { .. }
            | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternGlobalDecl { .. }
    )
}

/// Adds the activation events the ENTRY program's own interface declarations need.
///
/// The resolver strips an `include`d file's interfaces into activation events and the autoload pass
/// does the same for the files it splices, but the entry file's own declarations travel neither
/// path: they reach EIR as declarations, lower to a no-op, and leave the overlay cell that
/// `interface_exists()` reads at zero. `interface_exists()` therefore answered FALSE for an
/// interface declared in the very program being compiled, before and after its declaration.
///
/// Call this after `name_resolver::resolve` (namespaces flattened, every included file's
/// declaration already replaced) and before [`run_collecting_included_with_defines_and_sources`],
/// where the remaining top-level interface declarations are exactly the entry file's own.
pub fn activate_entry_interface_declarations(program: Program, entry_path: &Path) -> Program {
    // An INCLUDED file's declarations are hoisted to the entry program's top level while its
    // activation event stays behind at the include site, which may be nested, conditional, or
    // never reached. From here those declarations look exactly like the entry file's own, and the
    // only thing that tells them apart is the event they already carry. A second event
    // re-declares the interface at runtime -- `Cannot redeclare interface
    // Symfony\\...\\KernelInterface` on a front controller that `require_once`s a vendor interface
    // file -- and one added for a file the program never enters makes `interface_exists()` answer
    // true for an interface PHP never declared.
    let mut activated: HashSet<String> = HashSet::new();
    collect_activated_class_like_names(&program, &mut activated);
    activate_interface_declarations(program, entry_path, &|name| {
        !activated.contains(&crate::names::php_symbol_key(name))
    })
}

/// Collects every class-like name that already carries an activation event, at any depth.
///
/// The event can sit inside an include-once guard, a conditional, a loop, a function body, or a
/// method, so a top-level scan is not enough.
fn collect_activated_class_like_names(program: &[Stmt], out: &mut HashSet<String>) {
    // Closures included: a compiled DI container keeps `include_once` factories inside closure
    // literals, and an event there that this scan missed got a second, top-level event that
    // re-declared the interface at run time ("Cannot redeclare interface").
    crate::parser::ast::visit::walk_stmts(program, &mut |statement| {
        if let StmtKind::ClassLikeActivate { name, .. } = &statement.kind {
            out.insert(crate::names::php_symbol_key(name.trim_start_matches('\\')));
        }
    });
}

/// Adds the activation event every interface this file declares needs to be visible to
/// `interface_exists()`.
///
/// `interface_exists()` answers from a per-request activation cell, and NOTHING BUT a
/// `ClassLikeActivate` event ever writes that cell. `crate::resolver` emits one for each interface
/// it strips out of an `include`d file, but an autoloaded file never travels that path: it kept an
/// overlay cell that no statement could set, so `interface_exists()` answered false for the rest of
/// the program however early the declaration ran. `symfony/string`'s `AsciiSlugger.php` opens with
/// `if (!interface_exists(LocaleAwareInterface::class)) { throw ... }` and threw its
/// "symfony/translation-contracts is not installed" LogicException on boot because of it.
///
/// The declaration statement itself stays: unlike the include path, nothing has extracted it yet.
///
/// PHP early-binds an interface that extends nothing — it exists from the moment its file is
/// entered, ahead of the file's own statements. One that extends another interface binds where it
/// is written.
fn activate_interface_declarations(
    program: Program,
    path: &Path,
    accept: &dyn Fn(&str) -> bool,
) -> Program {
    use crate::parser::ast::ClassLikeKind;

    let mut early: Program = Vec::new();
    let mut rest: Program = Vec::with_capacity(program.len());
    for stmt in program {
        let StmtKind::InterfaceDecl {
            ref name,
            ref extends,
            ..
        } = stmt.kind
        else {
            rest.push(stmt);
            continue;
        };
        if !accept(name.trim_start_matches('\\')) {
            rest.push(stmt);
            continue;
        }
        let event = Stmt::new(
            StmtKind::ClassLikeActivate {
                name: name.clone(),
                kind: ClassLikeKind::Interface,
                source_path: path.to_path_buf(),
            },
            stmt.span,
        );
        if extends.is_empty() {
            early.push(event);
            rest.push(stmt);
        } else {
            rest.push(stmt);
            rest.push(event);
        }
    }
    early.extend(rest);
    early
}

/// Collects canonical declaration names associated with one physical source file.
fn declaration_source_files(program: &Program, source_file: &str) -> DeclarationSourceFiles {
    let mut sources = DeclarationSourceFiles::default();
    collect_declaration_source_files(program, source_file, &mut sources);
    sources
}

/// Recurses through transparent statement wrappers while recording declaration paths.
fn collect_declaration_source_files(
    program: &Program,
    source_file: &str,
    sources: &mut DeclarationSourceFiles,
) {
    for stmt in program {
        match &stmt.kind {
            crate::parser::ast::StmtKind::ClassDecl { name, .. }
            | crate::parser::ast::StmtKind::InterfaceDecl { name, .. }
            | crate::parser::ast::StmtKind::TraitDecl { name, .. }
            | crate::parser::ast::StmtKind::EnumDecl { name, .. }
            | crate::parser::ast::StmtKind::PackedClassDecl { name, .. } => {
                sources
                    .class_likes
                    .insert(
                        crate::names::php_symbol_key(name.trim_start_matches('\\')),
                        source_file.to_string(),
                    );
            }
            crate::parser::ast::StmtKind::FunctionDecl { name, .. } => {
                sources
                    .functions
                    .insert(
                        crate::names::php_symbol_key(name.trim_start_matches('\\')),
                        source_file.to_string(),
                    );
            }
            crate::parser::ast::StmtKind::NamespaceBlock { body, .. }
            | crate::parser::ast::StmtKind::Synthetic(body) => {
                collect_declaration_source_files(body, source_file, sources);
            }
            // A conditional declaration is still declared by this file (`symfony/security-core`
            // puts its `ExpressionLanguage` in the `else` of a `class_exists()` guard).
            crate::parser::ast::StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_declaration_source_files(then_body, source_file, sources);
                for (_, body) in elseif_clauses {
                    collect_declaration_source_files(body, source_file, sources);
                }
                if let Some(body) = else_body {
                    collect_declaration_source_files(body, source_file, sources);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod source_units_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// A class asked about only by non-autoloading `class_exists(X, false)` probes is probe-only,
    /// so the closed world keeps answering "not loaded" for it; any real use makes it a use.
    ///
    /// A `--web` build can leave the final program with only the `false` probe (the file-scope
    /// `class_exists(A, false) || class_exists(B) || class_exists(A)` of symfony/runtime loses
    /// its autoloading terms), and the class used to be classified as loaded: dotenv never ran.
    #[test]
    fn non_autoloading_probe_alone_makes_a_class_probe_only() {
        let classify = |source: &str| {
            let tokens = crate::lexer::tokenize(source).expect("tokenization should succeed");
            let parsed = crate::parser::parse(&tokens).expect("parsing should succeed");
            let resolved = crate::name_resolver::resolve(parsed).expect("resolution should succeed");
            probe_only_class_names(&resolved)
        };
        let only_false = classify(
            "<?php namespace App; var_dump(class_exists(Missing::class, false), class_exists('App\\Other', false));",
        );
        assert!(only_false.contains("App\\Missing"), "{only_false:?}");
        assert!(only_false.contains("App\\Other"), "{only_false:?}");
        let used = classify(
            "<?php namespace App; var_dump(class_exists(Missing::class, false)); new Missing();",
        );
        assert!(!used.contains("App\\Missing"), "{used:?}");
    }

    /// Verifies loaded declaration paths use PHP's case-insensitive symbol keys.
    #[test]
    fn declaration_source_files_normalize_class_and_function_names() {
        let tokens = crate::lexer::tokenize(
            "<?php namespace Demo; class Thing {} function execute(): void {}",
        )
        .expect("tokenization should succeed");
        let parsed = crate::parser::parse(&tokens).expect("parsing should succeed");
        let resolved = crate::name_resolver::resolve(parsed).expect("resolution should succeed");
        let sources = declaration_source_files(&resolved, "/tmp/Thing.php");
        assert_eq!(
            sources
                .class_likes
                .get(&crate::names::php_symbol_key("DEMO\\THING"))
                .map(String::as_str),
            Some("/tmp/Thing.php")
        );
        assert_eq!(
            sources
                .functions
                .get(&crate::names::php_symbol_key("DEMO\\EXECUTE"))
                .map(String::as_str),
            Some("/tmp/Thing.php")
        );
    }

    /// Verifies a throwing guard over an available class is folded away, and one over a missing
    /// class is kept. symfony/string's `AsciiSlugger.php` opens with the first shape; kept, it ran
    /// among the boot-time declarations of the demo console before its interface was active.
    #[test]
    fn satisfied_throwing_guard_is_folded_away() {
        let source = r#"<?php
namespace Demo;
if (!interface_exists(Present::class)) { throw new \LogicException('present'); }
if (!interface_exists(Missing::class)) { throw new \LogicException('missing'); }
final class Slugger {}
"#;
        let tokens = crate::lexer::tokenize(source).expect("tokenization should succeed");
        let parsed = crate::parser::parse(&tokens).expect("parsing should succeed");
        let resolved = crate::name_resolver::resolve(parsed).expect("resolution should succeed");
        let decide = |condition: &Expr| {
            existence_condition(
                condition,
                &|name: &str| name == "Demo\\Present",
                &|_, _| None,
                &|_, _| None,
            )
        };
        let folded = fold_existence_conditions_in(resolved, &decide);
        assert_eq!(file_scope_guard_classes(&folded), vec!["Demo\\Missing".to_string()]);
    }

    /// Verifies declaration binding dependencies autoload while callable signature types stay lazy.
    #[test]
    fn reference_points_defer_named_callable_signature_types() {
        let tokens = crate::lexer::tokenize(
            r#"<?php
namespace Fixtures;
class Carrier {
    private PropertyType $property;
    use PrimaryTrait, SecondaryTrait { PrimaryTrait::run insteadof SecondaryTrait; }
    public function method(MethodParam $param, MethodVariadic ...$rest): MethodReturn {}
}
function free(FunctionParam $param, FunctionVariadic ...$rest): FunctionReturn {}
$closure = function (ClosureParam $param, ClosureVariadic ...$rest): ClosureReturn {};
try {} catch (CaughtOne|CaughtTwo) {}
enum Choice implements EnumContract {
    use EnumTrait;
    public function accept(EnumMethodParam $param): EnumMethodReturn {}
}
"#,
        )
        .expect("tokenization should succeed");
        let parsed = crate::parser::parse(&tokens).expect("parsing should succeed");
        let resolved = crate::name_resolver::resolve(parsed).expect("resolution should succeed");
        let references = collect_reference_points(&resolved)
            .into_iter()
            .map(|(_, name)| name)
            .collect::<HashSet<_>>();
        let expected = [
            "Fixtures\\PrimaryTrait",
            "Fixtures\\SecondaryTrait",
            "Fixtures\\CaughtOne",
            "Fixtures\\CaughtTwo",
            "Fixtures\\EnumContract",
            "Fixtures\\EnumTrait",
        ];
        for name in expected {
            assert!(
                references.contains(name),
                "missing autoload reference {name}; collected {references:?}"
            );
        }
        for name in [
            "Fixtures\\PropertyType",
            "Fixtures\\MethodParam",
            "Fixtures\\MethodVariadic",
            "Fixtures\\MethodReturn",
            "Fixtures\\FunctionParam",
            "Fixtures\\FunctionVariadic",
            "Fixtures\\FunctionReturn",
            "Fixtures\\ClosureParam",
            "Fixtures\\ClosureVariadic",
            "Fixtures\\ClosureReturn",
            "Fixtures\\EnumMethodParam",
            "Fixtures\\EnumMethodReturn",
        ] {
            assert!(
                !references.contains(name),
                "signature type unexpectedly triggered autoload: {name}; collected {references:?}"
            );
        }
    }

    /// Verifies a `[Foo::class, 'method']` callable array is an autoload root, and a bare
    /// `Foo::class` still is not.
    ///
    /// The distinction is the whole point. PHP resolves `Foo::class` lexically and never consults
    /// the autoloader for it — a `sprintf()` argument naming a class from a package the app does
    /// not install must stay ignored, which was measured when collecting every `::class` pulled
    /// `DoctrineDbalAdapter` into a Symfony build off exactly such an argument. A CALLABLE ARRAY
    /// is built to be called, and calling it is what runs the loader.
    ///
    /// Twig's `EscaperExtension` is the case: `[FileExtensionEscapingStrategy::class, 'guess']` was
    /// the program's only reference to that class, and the build reported `Undefined class` for a
    /// file sitting in the same package.
    #[test]
    fn reference_points_include_callable_array_class_constants() {
        let tokens = crate::lexer::tokenize(
            r#"<?php
namespace Fixtures;
class Carrier {
    public function build(): array {
        $strategy = [EscapingStrategy::class, 'guess'];
        $pair = [DataOnly::class, 42];
        $message = \sprintf('%s is not enabled', MentionedOnly::class);
        return [$strategy, $pair, $message];
    }
}
"#,
        )
        .expect("tokenization should succeed");
        let parsed = crate::parser::parse(&tokens).expect("parsing should succeed");
        let resolved = crate::name_resolver::resolve(parsed).expect("resolution should succeed");
        let references = collect_reference_points(&resolved)
            .into_iter()
            .map(|(_, name)| name)
            .collect::<HashSet<_>>();
        assert!(
            references.contains("Fixtures\\EscapingStrategy"),
            "a callable array's class must autoload; collected {references:?}"
        );
        for name in ["Fixtures\\DataOnly", "Fixtures\\MentionedOnly"] {
            assert!(
                !references.contains(name),
                "`::class` outside a callable array must not autoload: {name}; collected {references:?}"
            );
        }
    }

    /// Verifies attribute-only class strings become autoload discovery roots, including nesting.
    #[test]
    fn reference_points_include_attribute_class_constant_dependencies() {
        let tokens = crate::lexer::tokenize(
            r#"<?php
namespace Fixtures;
#[Metadata(Dependency::class, nested: [NestedDependency::class])]
class Carrier {}
"#,
        )
        .expect("tokenization should succeed");
        let parsed = crate::parser::parse(&tokens).expect("parsing should succeed");
        let resolved = crate::name_resolver::resolve(parsed).expect("resolution should succeed");
        let references = collect_reference_points(&resolved)
            .into_iter()
            .map(|(_, name)| name)
            .collect::<HashSet<_>>();
        for name in [
            "Fixtures\\Metadata",
            "Fixtures\\Dependency",
            "Fixtures\\NestedDependency",
        ] {
            assert!(
                references.contains(name),
                "missing attribute dependency {name}; collected {references:?}"
            );
        }
    }
}
