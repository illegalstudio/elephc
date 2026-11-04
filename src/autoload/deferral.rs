//! Purpose:
//! Hands an autoloaded class file the type checker refuses back to the runtime autoloader,
//! instead of refusing the whole program.
//!
//! Called from:
//! - `crate::pipeline::compile()`, around the type-check phase.
//!
//! Key details:
//! - php parses and binds a class file only when something asks for the class. A closed-world
//!   compiler takes every class the program can reach, including ones a request never touches,
//!   so one construct it cannot type yet in one vendor class used to stop the build. Deferring
//!   the FILE keeps php's contract: the runtime autoloader includes it, the interpreter runs it.
//! - Only files the autoload pass loaded for a CLASS are deferrable. The entry program, its
//!   static includes, Composer's eager `files` and preloaded files still fail the build, because
//!   nothing at run time would load them again.
//! - An error is deferrable only when the checker named the declaration it was raised in
//!   (`CompileError::declaration`) and that declaration maps to such a file.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Program, StmtKind};

/// Looks up the file that declares class-like `name`.
///
/// An anonymous class is recorded under whatever name the pass that saw it had: the resolver
/// records it before namespaces are applied (`class@anonymous#6`), the name resolver afterwards
/// (`Twig\class@anonymous#6`). Its counter suffix is unique in the process, so the bare suffix is
/// tried when the qualified name is not found.
/// The file declaring class-like `name`, for callers outside the autoload pass.
pub fn class_file_path<'a>(class_files: &'a HashMap<String, String>, name: &str) -> Option<&'a String> {
    class_file(class_files, name)
}

pub(super) fn class_file<'a>(class_files: &'a HashMap<String, String>, name: &str) -> Option<&'a String> {
    let name = name.trim_start_matches('\\');
    class_files.get(&php_symbol_key(name)).or_else(|| {
        let (_, anonymous) = name.rsplit_once('\\')?;
        anonymous
            .starts_with("class@anonymous")
            .then(|| class_files.get(&php_symbol_key(anonymous)))
            .flatten()
    })
}

/// Maps each deferrable class file named by `error` (and its related errors) to the first
/// message raised in it. Errors that name no declaration, or a declaration outside
/// `deferrable_files`, are not returned: the caller still reports them.
pub fn deferrable_class_files(
    error: &CompileError,
    class_files: &HashMap<String, String>,
    deferrable_files: &HashSet<PathBuf>,
) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    for error in std::iter::once(error).chain(error.related.iter()) {
        let Some(declaration) = error.declaration.as_deref() else {
            continue;
        };
        let Some(path) = class_file(class_files, declaration) else {
            continue;
        };
        if !deferrable_files.contains(&canonical(Path::new(path))) {
            continue;
        }
        // The position is kept with the reason: the warning is the demo's gap list, and a reason
        // without a line ("Cannot index non-array") cannot be acted on in a 2,000-line file.
        files.entry(path.clone()).or_insert_with(|| {
            if error.span == crate::span::Span::dummy() {
                error.message.clone()
            } else {
                // A trait method spliced into the class keeps its own file's coordinates, so the
                // line is only actionable with that file's name (Carbon's traits are the case).
                let source = crate::span::Span::source_paths_snapshot()
                    .get(&error.span.source_id())
                    .filter(|source| canonical(Path::new(source.as_str())) != canonical(Path::new(path)))
                    .map(|source| format!("{source}:"))
                    .unwrap_or_default();
                format!("{source}{}:{}: {}", error.span.line, error.span.col, error.message)
            }
        });
    }
    files
}

/// The class file the declaration `error` names is in, canonicalized, if the map has it.
pub fn error_class_file(error: &CompileError, class_files: &HashMap<String, String>) -> Option<PathBuf> {
    let declaration = error.declaration.as_deref()?;
    class_file(class_files, declaration).map(|path| canonical(Path::new(path)))
}

/// Extends `deferred` with every deferrable file declaring a class-like that extends, implements
/// or uses one a deferred file declares, transitively, and returns the files it added.
///
/// A compiled class cannot inherit from one the runtime supplies, so a subclass has to follow
/// its parent out of the compiled world. Doing it here, rather than waiting for the checker to
/// refuse the orphan, also covers subclasses whose refusal names no declaration: an anonymous
/// `new class(...) extends BinaryOperatorExpressionParser` inside `twig/twig`'s `ExtensionSet.php`
/// is hoisted into that file, and its "unknown parent" refusal could not be mapped back to it.
pub fn propagate_to_subclasses(
    program: &Program,
    class_files: &HashMap<String, String>,
    deferrable: &HashSet<PathBuf>,
    deferred: &mut HashSet<PathBuf>,
) -> Vec<PathBuf> {
    let mut edges: Vec<(String, Vec<String>)> = Vec::new();
    collect_class_parents(program, &mut edges);
    let file_of =
        |name: &str| class_file(class_files, name).map(|path| canonical(Path::new(path)));
    let mut added = Vec::new();
    loop {
        let mut changed = false;
        for (class, parents) in &edges {
            let Some(file) = file_of(class) else {
                continue;
            };
            if deferred.contains(&file) || !deferrable.contains(&file) {
                continue;
            }
            let orphaned = parents
                .iter()
                .any(|parent| file_of(parent).is_some_and(|path| deferred.contains(&path)));
            if orphaned {
                deferred.insert(file.clone());
                added.push(file);
                changed = true;
            }
        }
        if !changed {
            return added;
        }
    }
}

/// Which constructor parameters of one class declaration are promoted by-reference properties.
struct ByRefConstructor {
    /// Parent class name, to find an inherited constructor.
    parent: Option<String>,
    /// `(parameter name, promoted to a by-reference property)` in declaration order, when the
    /// class declares its own constructor.
    params: Option<Vec<(String, bool)>>,
}

/// Collects, per class, the by-reference shape of its constructor and everything its methods
/// construct with place arguments.
fn collect_by_ref_constructors(
    program: &Program,
    constructors: &mut HashMap<String, ByRefConstructor>,
    callers: &mut Vec<(String, crate::ast_usage::Usage)>,
) {
    for stmt in program {
        match &stmt.kind {
            StmtKind::ClassDecl { name, extends, methods, properties, .. } => {
                // Only a parameter the constructor PROMOTES to a by-reference property keeps the
                // reference past the call. A by-reference parameter used during the call alone
                // (a transient out-parameter) is a different, narrower gap and stays compiled.
                let params = methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case("__construct"))
                    .map(|constructor| {
                        constructor
                            .params
                            .iter()
                            .map(|(param, _, _, by_ref)| {
                                let kept = *by_ref
                                    && properties.iter().any(|property| {
                                        property.is_promoted
                                            && property.by_ref
                                            && property.name == *param
                                    });
                                (param.clone(), kept)
                            })
                            .collect()
                    });
                constructors.insert(
                    php_symbol_key(name),
                    ByRefConstructor {
                        parent: extends.as_ref().map(|parent| parent.as_str().to_string()),
                        params,
                    },
                );
                callers.push((name.clone(), crate::ast_usage::collect_stmt(stmt)));
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => {
                collect_by_ref_constructors(body, constructors, callers)
            }
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                collect_by_ref_constructors(then_body, constructors, callers);
                for (_, body) in elseif_clauses {
                    collect_by_ref_constructors(body, constructors, callers);
                }
                if let Some(body) = else_body {
                    collect_by_ref_constructors(body, constructors, callers);
                }
            }
            _ => {}
        }
    }
}

/// Resolves the constructor parameters `class` runs, walking up to the nearest declared one.
fn effective_constructor_params<'a>(
    constructors: &'a HashMap<String, ByRefConstructor>,
    class: &str,
) -> Option<&'a Vec<(String, bool)>> {
    let mut current = php_symbol_key(class.trim_start_matches('\\'));
    for _ in 0..64 {
        let info = constructors.get(&current)?;
        if let Some(params) = &info.params {
            return Some(params);
        }
        current = php_symbol_key(info.parent.as_deref()?.trim_start_matches('\\'));
    }
    None
}

/// Defers every compiled class that hands a PLACE (a property, a static property or an array
/// element) to a constructor parameter another class promotes to a by-reference property, together with that class,
/// and returns the files it added.
///
/// A native construction passing such a place copies it into a temporary: the callee's reference
/// never reaches the caller's storage, so a constructor that keeps it
/// (`__construct(private array &$elements)`, then `$this->elements[] = $x`) writes into a copy
/// and the owner never sees it -- a silent wrong answer. The interpreter implements references,
/// so the pair is left to the runtime autoloader and runs there, the way php runs it. A declared
/// instance property and its elements are no longer such places: the checker hands the callee the
/// property's own cell (`Checker::promote_by_ref_property_args`), so `ast_usage` does not record
/// them, and Symfony's `OrderedHashMap` / `OrderedHashMapIterator` pair stays compiled.
///
/// Both files have to be deferrable: a pair the autoloader cannot hand back is left as it is,
/// because deferring only one half would turn a by-value copy into a call across the compiled
/// and interpreted worlds that has no reference support at all.
pub fn propagate_by_ref_place_constructions(
    program: &Program,
    class_files: &HashMap<String, String>,
    deferrable: &HashSet<PathBuf>,
    deferred: &mut HashSet<PathBuf>,
) -> Vec<PathBuf> {
    let mut constructors = HashMap::new();
    let mut callers = Vec::new();
    collect_by_ref_constructors(program, &mut constructors, &mut callers);
    let file_of = |name: &str| class_file(class_files, name).map(|path| canonical(Path::new(path)));
    let mut added = Vec::new();
    loop {
        let mut changed = false;
        for (caller, usage) in &callers {
            let Some(caller_file) = file_of(caller) else {
                continue;
            };
            if deferred.contains(&caller_file) || !deferrable.contains(&caller_file) {
                continue;
            }
            for construction in &usage.place_constructions {
                let Some(params) = effective_constructor_params(&constructors, &construction.class_key)
                else {
                    continue;
                };
                let hits_by_ref = construction.places.iter().any(|place| match &place.name {
                    Some(name) => params.iter().any(|(param, by_ref)| *by_ref && param == name),
                    None => params.get(place.position).is_some_and(|(_, by_ref)| *by_ref),
                });
                if !hits_by_ref {
                    continue;
                }
                let Some(callee_file) = file_of(&construction.class_key) else {
                    continue;
                };
                if !deferrable.contains(&callee_file) {
                    continue;
                }
                for file in [caller_file.clone(), callee_file] {
                    if deferred.insert(file.clone()) {
                        added.push(file);
                        changed = true;
                    }
                }
                break;
            }
        }
        if !changed {
            return added;
        }
    }
}

/// Every class-like declaration of `program` with the names it extends, implements or uses.
///
/// Taken BEFORE any deferral removes declarations: once a deferred class is gone from the
/// program, nothing else records which compiled classes it descended from, and
/// `runtime_faults` needs that to know which receivers may hold a deferred subclass.
pub fn class_parent_edges(program: &Program) -> Vec<(String, Vec<String>)> {
    let mut edges = Vec::new();
    collect_class_parents(program, &mut edges);
    edges
}

/// Collects each class-like declaration with the names it extends, implements or uses.
fn collect_class_parents(program: &Program, out: &mut Vec<(String, Vec<String>)>) {
    for stmt in program {
        match &stmt.kind {
            StmtKind::ClassDecl { name, extends, implements, trait_uses, .. } => {
                let mut parents: Vec<String> =
                    extends.iter().map(|name| name.as_str().to_string()).collect();
                parents.extend(implements.iter().map(|name| name.as_str().to_string()));
                parents.extend(
                    trait_uses
                        .iter()
                        .flat_map(|trait_use| trait_use.trait_names.iter())
                        .map(|name| name.as_str().to_string()),
                );
                out.push((name.clone(), parents));
            }
            StmtKind::InterfaceDecl { name, extends, .. } => {
                out.push((
                    name.clone(),
                    extends.iter().map(|name| name.as_str().to_string()).collect(),
                ));
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => collect_class_parents(body, out),
            // `if (!class_exists(Base::class)) { throw …; } else { class X extends Base {} }` is
            // how `symfony/security-core` declares its `ExpressionLanguage`.
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                collect_class_parents(then_body, out);
                for (_, body) in elseif_clauses {
                    collect_class_parents(body, out);
                }
                if let Some(body) = else_body {
                    collect_class_parents(body, out);
                }
            }
            _ => {}
        }
    }
}

/// The refusals the checker's inheritance flattening would raise for classes whose PARENT no
/// declaration of the program and no builtin of the profile provides, as that flattening raises
/// them (message, and the child class as the declaration), or `None` when there is none.
///
/// Such a refusal costs a whole type check: the check runs, fails on it, the file is deferred,
/// and the program is checked again from scratch (a second 20 s check of a Symfony build, for
/// `framework-bundle`'s `KernelBrowser` whose `HttpKernelBrowser` parent is not installed). The
/// check loop asks this first so the deferral round costs a program walk instead.
///
/// Conservative by design: a parent this cannot rule out (a builtin, a `class_alias()` target,
/// any program with a non-literal `class_alias()`) is left to the checker, which still decides.
pub fn unknown_parent_refusals(program: &Program) -> Option<CompileError> {
    let declared: HashSet<String> = super::walk::collect_declared_fqns(program)
        .iter()
        .map(|name| php_symbol_key(name.trim_start_matches('\\')))
        .collect();
    let mut aliases = HashSet::new();
    let mut opaque_alias = false;
    crate::parser::ast::visit::walk_nodes(program, &mut |node| {
        let crate::parser::ast::visit::Node::Expr(expr) = node else { return };
        let crate::parser::ast::ExprKind::FunctionCall { name, args } = &expr.kind else { return };
        if !name.as_str().trim_start_matches('\\').eq_ignore_ascii_case("class_alias") {
            return;
        }
        match super::resolved_class_alias_args(args) {
            Some((_, alias)) => {
                aliases.insert(php_symbol_key(&alias));
            }
            None => opaque_alias = true,
        }
    });
    if opaque_alias {
        return None;
    }
    let mut refusals = Vec::new();
    let mut edges: Vec<(String, String)> = collect_extends(program).into_iter().collect();
    edges.sort();
    for (class, parent) in edges {
        let parent = parent.trim_start_matches('\\');
        let key = php_symbol_key(parent);
        if declared.contains(&key)
            || aliases.contains(&key)
            || crate::types::builtin_classes::lookup_profile_class(parent).is_some()
        {
            continue;
        }
        refusals.push(
            CompileError::new(
                crate::span::Span::dummy(),
                &format!("Unknown class referenced during inheritance flattening: {parent}"),
            )
            .within_declaration(&class),
        );
    }
    (!refusals.is_empty()).then(|| CompileError::from_many(refusals))
}

/// Class name -> the class it extends, for every class declaration of the program.
fn collect_extends(program: &Program) -> HashMap<String, String> {
    fn walk(program: &Program, out: &mut HashMap<String, String>) {
        for stmt in program {
            match &stmt.kind {
                StmtKind::ClassDecl { name, extends: Some(parent), .. } => {
                    out.insert(name.clone(), parent.as_str().to_string());
                }
                StmtKind::NamespaceBlock { body, .. }
                | StmtKind::Synthetic(body)
                | StmtKind::IncludeOnceGuard { body, .. } => walk(body, out),
                StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                    walk(then_body, out);
                    for (_, body) in elseif_clauses {
                        walk(body, out);
                    }
                    if let Some(body) = else_body {
                        walk(body, out);
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = HashMap::new();
    walk(program, &mut out);
    out
}

/// Removes every class-like declaration a deferred file made, and the activation events the
/// autoload pass placed for them, so the checker and the backend no longer see the classes.
pub fn remove_deferred_declarations(
    program: Program,
    class_files: &HashMap<String, String>,
    deferred: &HashSet<PathBuf>,
) -> Program {
    let is_deferred = |name: &str| {
        class_file(class_files, name)
            .is_some_and(|path| deferred.contains(&canonical(Path::new(path))))
    };
    filter_statements(program, &is_deferred, deferred)
}

/// Filters one statement list, descending into the file-level wrappers the autoload pass and
/// the resolver put around spliced files.
fn filter_statements(
    program: Program,
    is_deferred: &dyn Fn(&str) -> bool,
    deferred: &HashSet<PathBuf>,
) -> Program {
    let mut kept = Vec::with_capacity(program.len());
    for mut stmt in program {
        match &mut stmt.kind {
            StmtKind::ClassDecl { name, .. }
            | StmtKind::InterfaceDecl { name, .. }
            | StmtKind::TraitDecl { name, .. }
            | StmtKind::EnumDecl { name, .. }
            | StmtKind::PackedClassDecl { name, .. } => {
                if is_deferred(name) {
                    continue;
                }
            }
            StmtKind::ClassLikeActivate { source_path, .. } => {
                if deferred.contains(&canonical(source_path)) {
                    continue;
                }
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => {
                *body = filter_statements(std::mem::take(body), is_deferred, deferred);
            }
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                *then_body = filter_statements(std::mem::take(then_body), is_deferred, deferred);
                for (_, body) in elseif_clauses.iter_mut() {
                    *body = filter_statements(std::mem::take(body), is_deferred, deferred);
                }
                if let Some(body) = else_body {
                    *body = filter_statements(std::mem::take(body), is_deferred, deferred);
                }
            }
            _ => {}
        }
        kept.push(stmt);
    }
    kept
}

/// Canonicalizes a path for comparison, keeping it as written when it cannot be resolved.
fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Returns the `eager_function_files` key of one EIR function name: the public qualified name,
/// with a conditional-declaration variant's `__elephc_conditional_fn_<hash>_` prefix removed.
fn eager_function_key(name: &str) -> String {
    const VARIANT: &str = "__elephc_conditional_fn_";
    let (namespace, local) = match name.rsplit_once('\\') {
        Some((namespace, local)) => (Some(namespace), local),
        None => (None, name),
    };
    let local = local
        .strip_prefix(VARIANT)
        .and_then(|rest| rest.split_once('_'))
        .map(|(_hash, public)| public)
        .unwrap_or(local);
    let public = match namespace {
        Some(namespace) => format!("{namespace}\\{local}"),
        None => local.to_string(),
    };
    crate::names::php_symbol_key(public.trim_start_matches('\\'))
}

/// Environment variable carrying, into a re-run of the compiler, the class files an earlier
/// round of the same build already handed to the runtime (newline-separated paths).
const DEFERRED_FILES_ENV: &str = "ELEPHC_DEFERRED_CLASS_FILES";

/// Environment variable carrying the deferred files that do not count against a closed world
/// (only probed, or refused by php itself): the classification rests on what the round that
/// found them saw, and a re-run starts from an empty set.
const HARMLESS_DEFERRED_FILES_ENV: &str = "ELEPHC_HARMLESS_DEFERRED_CLASS_FILES";

/// Environment variable counting re-runs, so a build that keeps finding new files still ends.
const DEFERRAL_ROUND_ENV: &str = "ELEPHC_DEFERRAL_ROUND";

/// The most re-runs one build may take before the backend's refusal is reported as it is.
const MAX_DEFERRAL_ROUNDS: u32 = 6;

/// The class files an earlier round of this build handed to the runtime, if any.
pub fn inherited_deferred_files() -> Vec<PathBuf> {
    paths_from_env(DEFERRED_FILES_ENV)
}

/// The inherited files that do not count against a closed world.
pub fn inherited_harmless_deferred_files() -> Vec<PathBuf> {
    paths_from_env(HARMLESS_DEFERRED_FILES_ENV)
}

/// Reads one newline-separated path list from the environment.
fn paths_from_env(name: &str) -> Vec<PathBuf> {
    std::env::var(name)
        .map(|joined| {
            joined
                .lines()
                .filter(|line| !line.is_empty())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

/// What the backend needs to hand a class file it cannot lower back to the runtime.
///
/// Only the backend knows which bodies it cannot emit, and it learns that after the checker has
/// accepted the program. A body belongs to a class; when that class came from a deferrable file,
/// the build is run again with the file handed to the runtime autoloader from the start.
pub struct BackendDeferral {
    /// EIR function name to the deferrable class file its body belongs to.
    pub body_files: HashMap<String, PathBuf>,
    /// The files this round has already handed back.
    pub already_deferred: HashSet<PathBuf>,
    /// The already deferred files that do not count against a closed world.
    pub harmless_deferred: HashSet<PathBuf>,
    /// For a body that cannot be handed back, the class it belongs to and why, for the report.
    pub owners: HashMap<String, String>,
}

impl BackendDeferral {
    /// Maps every body of `module` to the deferrable class file of the class it belongs to.
    ///
    /// A method is named `Class::method`; a closure carries the class it was written in
    /// (`lexical_class`); a property-initializer thunk is `_class_propinit_<class id>`. A body
    /// whose class is not deferrable (compiler-generated reflection bodies, the entry program)
    /// is left out, and its refusal is still reported.
    pub fn for_module(
        module: &crate::ir::Module,
        class_files: &HashMap<String, String>,
        deferrable: &HashSet<PathBuf>,
        already_deferred: &HashSet<PathBuf>,
        harmless_deferred: &HashSet<PathBuf>,
        eager_function_files: &HashMap<String, PathBuf>,
    ) -> Self {
        let class_by_id: HashMap<u64, &str> = module
            .class_infos
            .iter()
            .map(|(name, info)| (info.class_id, name.as_str()))
            .collect();
        let mut body_files = HashMap::new();
        let mut owners = HashMap::new();
        // One `realpath` per distinct class file: it was asked per method body and again per
        // class, a file-system walk each (most of the backend's `getattrlist` time on Symfony).
        let mut canonical_files: HashMap<String, PathBuf> = HashMap::new();
        let mut canonical_file = |path: &str| -> PathBuf {
            canonical_files
                .entry(path.to_string())
                .or_insert_with(|| canonical(Path::new(path)))
                .clone()
        };
        for function in module
            .functions
            .iter()
            .chain(module.class_methods.iter())
            .chain(module.closures.iter())
            .chain(module.fiber_wrappers.iter())
            .chain(module.callback_wrappers.iter())
        {
            let owner = if let Some(id) = function.name.strip_prefix("_class_propinit_") {
                id.parse::<u64>().ok().and_then(|id| class_by_id.get(&id).map(|name| name.to_string()))
            } else if let Some((class, _)) = function.name.rsplit_once("::") {
                Some(class.to_string())
            } else {
                function.lexical_class.clone()
            };
            let Some(owner) = owner else {
                // A function an eager Composer `files` entry declares has no class, but it has a
                // file the runtime requires at startup anyway: hand THAT file back. A conditional
                // declaration (`if (!function_exists('f'))`) reaches here under its variant name
                // `Ns\__elephc_conditional_fn_<hash>_f`, whose public name is `Ns\f`.
                if let Some(path) = eager_function_files.get(&eager_function_key(&function.name)) {
                    body_files.insert(function.name.clone(), path.clone());
                }
                continue;
            };
            let Some(path) = class_file(class_files, &owner) else {
                owners.insert(function.name.clone(), format!("{owner} (no recorded file)"));
                continue;
            };
            let path = canonical_file(path);
            if deferrable.contains(&path) {
                body_files.insert(function.name.clone(), path);
            } else {
                owners.insert(
                    function.name.clone(),
                    format!("{owner} in {} (not an autoloaded or preloaded class file)", path.display()),
                );
            }
        }
        // A static-property default is written by the entry program's prologue, and a refused
        // one is recorded against `Class::$prop` (`codegen::static_property_owner_body`). PHP
        // evaluates that default when the class is first used, so the declaring file can go to
        // the runtime autoloader like any refused method body.
        for (class_name, info) in &module.class_infos {
            let Some(path) = class_file(class_files, class_name) else {
                continue;
            };
            let path = canonical_file(path);
            if !deferrable.contains(&path) {
                continue;
            }
            for (property, _) in &info.static_properties {
                let declared_here = info
                    .static_property_declaring_classes
                    .get(property)
                    .is_none_or(|declaring| declaring == class_name);
                if declared_here {
                    body_files.insert(
                        crate::codegen::static_property_owner_body(class_name, property),
                        path.clone(),
                    );
                }
            }
        }
        Self {
            body_files,
            already_deferred: already_deferred.clone(),
            harmless_deferred: harmless_deferred.clone(),
            owners,
        }
    }

    /// Re-runs the build with the files the refused bodies belong to handed to the runtime, and
    /// exits with the re-run's status. Returns only when there is nothing new to hand back or the
    /// round limit is reached, so the caller reports the refusal as it is.
    pub fn rerun_without_refused_bodies(&self, failed_bodies: &[(String, String)]) {
        let round: u32 = std::env::var(DEFERRAL_ROUND_ENV)
            .ok()
            .and_then(|round| round.parse().ok())
            .unwrap_or(0);
        if round >= MAX_DEFERRAL_ROUNDS {
            return;
        }
        let mut new_files: BTreeMap<PathBuf, (&str, &str)> = BTreeMap::new();
        let mut kept = 0usize;
        for (body, detail) in failed_bodies {
            match self.body_files.get(body) {
                Some(path) if !self.already_deferred.contains(path) => {
                    new_files
                        .entry(path.clone())
                        .or_insert((body.as_str(), detail.as_str()));
                }
                _ => kept += 1,
            }
        }
        if new_files.is_empty() {
            return;
        }
        for (path, (body, detail)) in &new_files {
            crate::errors::report_warning(&crate::errors::CompileWarning::new(
                crate::span::Span::dummy(),
                &format!(
                    "autoload: '{}' is not compiled (the backend cannot lower `{}`: {}); its \
                     classes are loaded at run time by the autoloader instead",
                    path.display(),
                    body,
                    detail
                ),
            ));
        }
        if kept > 0 {
            // Bodies no runtime autoloader can supply (generated reflection helpers, the entry
            // program) will still fail the next round; name them now so the log says why.
            for (body, detail) in failed_bodies {
                if !self.body_files.contains_key(body) {
                    let owner = self
                        .owners
                        .get(body)
                        .map(String::as_str)
                        .unwrap_or("no owning class");
                    eprintln!("note: backend refusal not deferrable: {body} [{owner}]: {detail}");
                }
            }
        }
        let all: Vec<String> = self
            .already_deferred
            .iter()
            .chain(new_files.keys())
            .map(|path| path.display().to_string())
            .collect();
        let harmless: Vec<String> =
            self.harmless_deferred.iter().map(|path| path.display().to_string()).collect();
        crate::progress::clear();
        eprintln!(
            "note: recompiling with {} class file(s) left to the runtime autoloader (round {})",
            all.len(),
            round + 1
        );
        let Ok(executable) = std::env::current_exe() else {
            return;
        };
        let mut command = std::process::Command::new(executable);
        command
            .args(std::env::args_os().skip(1))
            .env(DEFERRED_FILES_ENV, all.join("\n"))
            .env(HARMLESS_DEFERRED_FILES_ENV, harmless.join("\n"))
            .env(DEFERRAL_ROUND_ENV, (round + 1).to_string());
        // Replace this process rather than wait on a child: a waiting parent kept its whole
        // compiled program resident (~10 GB on the Symfony demo) beside the re-run's, which
        // doubled the build's peak memory and pushed the machine into reclaim. `exec` returns
        // only when it failed, and the spawn below is then the fallback.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let _ = command.exec();
        }
        let status = command.status();
        match status {
            Ok(status) => std::process::exit(status.code().unwrap_or(1)),
            Err(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses and name-resolves a small program for the removal tests.
    fn program(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        let parsed = crate::parser::parse(&tokens).expect("parse");
        crate::name_resolver::resolve(parsed).expect("resolve")
    }

    /// The precheck raises the inheritance-flattening refusal the checker would, attributed to
    /// the child, for a parent nothing provides; a declared, builtin or aliased parent is not
    /// refused, and a non-literal `class_alias()` leaves every decision to the checker.
    #[test]
    fn unknown_parent_refusals_mirror_the_flattening_refusal() {
        let refused = unknown_parent_refusals(&program(
            "<?php namespace App; class Browser extends \\Missing\\HttpKernelBrowser {} class Ok extends \\Exception {} class Base {} class Child extends Base {}",
        ))
        .expect("a missing parent is refused");
        assert_eq!(refused.declaration.as_deref(), Some("App\\Browser"));
        assert!(refused.related.is_empty(), "{:?}", refused.related);
        assert_eq!(
            refused.message,
            "Unknown class referenced during inheritance flattening: Missing\\HttpKernelBrowser"
        );
        assert!(unknown_parent_refusals(&program(
            "<?php class_alias('Real', 'Alias'); class Real {} class Child extends Alias {}",
        ))
        .is_none());
        assert!(unknown_parent_refusals(&program(
            "<?php class_alias('Real', $name); class Child extends Missing {}",
        ))
        .is_none());
    }

    /// Only errors naming a declaration from a deferrable file are returned, keyed by file.
    #[test]
    fn deferrable_files_come_from_attributed_errors_only() {
        let dir = std::env::temp_dir().join(format!("elephc-deferral-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let class_file = dir.join("Broken.php");
        std::fs::write(&class_file, "<?php").expect("write");
        let class_files = HashMap::from([(
            php_symbol_key("App\\Broken"),
            class_file.display().to_string(),
        )]);
        let deferrable = HashSet::from([canonical(&class_file)]);

        let mut primary = CompileError::new(crate::span::Span::dummy(), "first");
        primary.declaration = Some("App\\Broken".to_string());
        let mut entry = CompileError::new(crate::span::Span::dummy(), "entry");
        entry.declaration = Some("App\\Entry".to_string());
        primary.related.push(entry);
        primary
            .related
            .push(CompileError::new(crate::span::Span::dummy(), "unattributed"));

        let files = deferrable_class_files(&primary, &class_files, &deferrable);
        assert_eq!(files.len(), 1);
        assert_eq!(files.values().next().map(String::as_str), Some("first"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Removing a deferred file drops its classes and keeps every other statement.
    #[test]
    fn removal_drops_only_the_deferred_classes() {
        let deferred_path = PathBuf::from("/nonexistent/Deferred.php");
        let class_files = HashMap::from([(
            php_symbol_key("Deferred"),
            deferred_path.display().to_string(),
        )]);
        let deferred = HashSet::from([deferred_path]);
        let kept = remove_deferred_declarations(
            program("<?php class Deferred {} class Kept {} echo 1;"),
            &class_files,
            &deferred,
        );
        let names: Vec<&str> = kept
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::ClassDecl { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, vec!["Kept"]);
        assert_eq!(kept.len(), 2, "the echo statement stays");
    }

    /// A refused body is traced back to the eager file declaring its PUBLIC function name, both
    /// for a conditional-declaration variant and for a plain function, namespaced or global.
    #[test]
    fn eager_function_key_recovers_the_public_name() {
        assert_eq!(
            eager_function_key("Psy\\__elephc_conditional_fn_c10072783125ac47_info"),
            "psy\\info"
        );
        assert_eq!(
            eager_function_key("__elephc_conditional_fn_0123456789abcdef_str_contains"),
            "str_contains"
        );
        assert_eq!(eager_function_key("Laravel\\Prompts\\text"), "laravel\\prompts\\text");
        assert_eq!(eager_function_key("helper"), "helper");
    }
}
