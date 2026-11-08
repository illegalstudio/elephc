//! Purpose:
//! Collects declared and referenced fully-qualified class-like names from the AST.
//! Gives the autoload pass the missing symbols it can try to resolve from disk.
//!
//! Called from:
//! - `crate::autoload::run()`
//!
//! Key details:
//! - Literal `class_exists(..., true)` shapes are treated as compile-time autoload demands.
//! - Dynamic autoload flags are not guessed because the checker rejects them in AOT mode.

use std::collections::{HashMap, HashSet};

use crate::parser::ast::{
    AttributeGroup, CallableTarget, CatchClause, ClassConst, ClassMethod, ClassProperty, Expr,
    ExprKind, Program, StaticReceiver, Stmt, StmtKind, TraitAdaptation, TraitUse, TypeExpr,
};

/// Stable l-value shapes whose deterministic class-string assignments can seed dynamic `new`.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum DynamicClassTarget {
    Variable(String),
    ArrayAccess {
        array: Box<DynamicClassTarget>,
        index: DynamicClassIndex,
    },
}

/// Literal array indexes accepted in a stable dynamic-class target.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum DynamicClassIndex {
    String(String),
    Int(i64),
}

/// Per-statement reference collection state, including possible class-string assignments.
struct ReferenceSet {
    names: HashSet<String>,
    /// Names reached by something OTHER than a class-like existence probe.
    ///
    /// PHP loads a class when code USES it; `class_exists()` merely asks whether it can be
    /// loaded. Separating the two is what lets [`probe_only_class_names`] tell a class the
    /// program genuinely depends on from one the closed world carries only so a probe can be
    /// answered -- and the latter must still report "not loaded" to `class_exists($n, false)`.
    non_probe: HashSet<String>,
    /// Names probed by a `class_exists(X, false)`-style call, which never autoloads: kept apart
    /// from `names` because they are not autoload demands, only probe-only candidates.
    probe_noload: HashSet<String>,
    dynamic_class_defaults: HashMap<DynamicClassTarget, HashSet<String>>,
}

impl ReferenceSet {
    /// Creates an empty reference set seeded by the current class-string assignment facts.
    fn new(dynamic_class_defaults: &HashMap<DynamicClassTarget, HashSet<String>>) -> Self {
        Self::owning(dynamic_class_defaults.clone())
    }

    /// Creates an empty reference set that takes over the class-string assignment facts.
    fn owning(dynamic_class_defaults: HashMap<DynamicClassTarget, HashSet<String>>) -> Self {
        Self {
            names: HashSet::new(),
            non_probe: HashSet::new(),
            probe_noload: HashSet::new(),
            dynamic_class_defaults,
        }
    }

    /// Inserts one canonical class-like name into this statement's autoload demands.
    fn insert(&mut self, name: String) {
        self.non_probe.insert(name.clone());
        self.names.insert(name);
    }

    /// Records a name asked about by a probe that does not autoload.
    fn insert_probe_noload(&mut self, name: String) {
        self.probe_noload.insert(name);
    }

    /// Inserts one name demanded ONLY by a class-like existence probe.
    fn insert_probe(&mut self, name: String) {
        self.names.insert(name);
    }
}

/// Returns the class-like names the program references ONLY through existence probes.
///
/// `class_exists($name, false)` asks whether a class is already LOADED, not whether it could be.
/// A closed-world build declares everything it compiled from the first instruction, so a class
/// the compiler pulled in solely to answer `class_exists(X::class)` answered "loaded" where php
/// answers "not loaded" -- and `symfony/runtime` reads exactly that difference to decide whether
/// `symfony/dotenv` is installed:
///
/// ```php
/// class_exists(MissingDotenv::class, false) || class_exists(Dotenv::class) || class_exists(MissingDotenv::class);
/// // ...
/// if (... && !class_exists(MissingDotenv::class, false)) { $dotenv->bootEnv(...); }
/// ```
///
/// php never loads `MissingDotenv` when dotenv IS installed (the second probe short-circuits), so
/// the guard passes and `.env` is read. elephc carried the class, answered `true`, skipped
/// `bootEnv()`, and every `%env(...)%` parameter then failed to resolve.
///
/// A name reached by anything else -- `new`, `extends`, a type, a static call, a catch -- is NOT
/// returned: the program depends on it, so php would have loaded it too.
pub(super) fn probe_only_class_names(program: &Program) -> HashSet<String> {
    let mut refs = ReferenceSet::new(&HashMap::new());
    for stmt in program {
        collect_refs_stmt(stmt, &mut refs);
    }
    let non_probe = refs.non_probe;
    // A `class_exists(X, false)` probe never loads X, so it adds nothing to the autoload demands;
    // but a class the build carries anyway (another probe, or the preload's own closure) is
    // answered "not loaded" by exactly this probe, and only if no use of it exists. The final
    // program can retain ONLY the `false` probe: a web build folds `class_exists(A, false) ||
    // class_exists(B) || class_exists(A)` to its first two terms, and without this the
    // loading probe that classified A was gone and A answered "loaded".
    refs.names
        .into_iter()
        .chain(refs.probe_noload)
        .filter(|name| !non_probe.contains(name))
        .collect()
}

/// Collect all declared fully-qualified class-like names from the program.
pub(super) fn collect_declared_fqns(program: &Program) -> HashSet<String> {
    let mut out = HashSet::new();
    for stmt in program {
        collect_declared_in_stmt(stmt, &mut out);
    }
    out
}

/// Recurse into a statement to collect declared class names.
fn collect_declared_in_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
    match &stmt.kind {
        StmtKind::ClassDecl { name, .. }
        | StmtKind::InterfaceDecl { name, .. }
        | StmtKind::TraitDecl { name, .. }
        | StmtKind::EnumDecl { name, .. }
        | StmtKind::PackedClassDecl { name, .. } => {
            out.insert(name.trim_start_matches('\\').to_string());
        }
        // A spliced include's body runs unconditionally where it stands, so what it declares is
        // declared: `UseUse.php`'s `require __DIR__.'/../UseItem.php'` declares `UseItem`.
        StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => {
            for inner in body {
                collect_declared_in_stmt(inner, out);
            }
        }
        _ => {}
    }
}

/// Returns the first class-like one loaded file declares only inside a file-scope conditional.
///
/// php declares such a class when the `if` holding it RUNS, which for a class file is the moment
/// the autoloader includes it:
///
/// ```php
/// if (interface_exists(SimpleCacheInterface::class)) {
///     class InvalidArgumentException extends \InvalidArgumentException implements Psr6, Simple {}
/// } else {
///     class InvalidArgumentException extends \InvalidArgumentException implements Psr6 {}
/// }
/// ```
///
/// The compiled world has no runtime class declaration, so the branches lose their classes while
/// the file is still claimed as included, and the autoloader then skips the include: the class is
/// declared by nobody. A condition the language profile decides has already been folded away by
/// `fold_file_scope_profile_conditions`, so what is left here genuinely depends on run time.
/// Transparent wrappers (`namespace { }`, synthetic blocks, include guards) are walked through;
/// function and method bodies are not file scope and are not visited.
pub(super) fn first_conditionally_declared_class_like(program: &Program) -> Option<String> {
    /// Collects every class-like declared at any depth of these conditional statement lists.
    fn declared_under(stmts: &[Stmt]) -> Option<String> {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::ClassDecl { name, .. }
                | StmtKind::InterfaceDecl { name, .. }
                | StmtKind::TraitDecl { name, .. }
                | StmtKind::EnumDecl { name, .. } => {
                    return Some(name.trim_start_matches('\\').to_string());
                }
                _ => {
                    if let Some(name) = conditional_bodies(stmt)
                        .into_iter()
                        .chain(transparent_body(stmt))
                        .find_map(declared_under)
                    {
                        return Some(name);
                    }
                }
            }
        }
        None
    }

    /// The statement lists a file-scope statement runs only under a run-time condition.
    fn conditional_bodies(stmt: &Stmt) -> Vec<&[Stmt]> {
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
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => bodies.push(body),
            StmtKind::Switch { cases, default, .. } => {
                bodies.extend(cases.iter().map(|(_, body)| body.as_slice()));
                if let Some(body) = default {
                    bodies.push(body);
                }
            }
            _ => {}
        }
        bodies
    }

    /// The statement list of a wrapper that runs its body unconditionally.
    fn transparent_body(stmt: &Stmt) -> Option<&[Stmt]> {
        match &stmt.kind {
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => Some(body),
            _ => None,
        }
    }

    /// Walks file-scope statements, through transparent wrappers, into conditional bodies.
    fn file_scope(stmts: &[Stmt]) -> Option<String> {
        for stmt in stmts {
            if let Some(body) = transparent_body(stmt) {
                if let Some(name) = file_scope(body) {
                    return Some(name);
                }
                continue;
            }
            if let Some(name) = conditional_bodies(stmt).into_iter().find_map(declared_under) {
                return Some(name);
            }
        }
        None
    }

    file_scope(program)
}

/// Returns the first call a class file's FILE-SCOPE code makes, other than the existence probes,
/// alias and constant definitions every class file may run without observable effect.
///
/// php executes that code when the autoloader includes the file. Declarations, namespace and
/// import statements, plain assignments and the allowed calls are the same whenever they run.
/// `echo` counts as a call: it writes output. Function and method bodies are not file scope and
/// are not visited; a condition and the branches it guards are.
/// Removes the file-scope `trigger_deprecation(...)` statements of a class file, and says whether
/// there were any.
///
/// php runs them when the autoloader first includes the file. A closed-world build has no
/// autoloader to defer the file to, and running them at boot would report a deprecation for a
/// class nothing used; the notice is a silenced `E_USER_DEPRECATED` that only an error handler
/// records, so the closed world compiles the class without it (symfony/http-kernel's
/// `Config/FileLocator.php` and `DependencyInjection/ServicesResetterInterface.php`).
pub(super) fn strip_file_scope_deprecations(program: &mut Program) -> bool {
    fn is_deprecation(stmt: &Stmt) -> bool {
        matches!(
            &stmt.kind,
            StmtKind::ExprStmt(Expr { kind: ExprKind::FunctionCall { name, .. }, .. })
                // Unqualified in a namespace, the call falls back to the global function.
                if name.last_segment().is_some_and(|base| base.eq_ignore_ascii_case("trigger_deprecation"))
        )
    }
    let mut stripped = false;
    program.retain(|stmt| {
        let deprecation = is_deprecation(stmt);
        stripped |= deprecation;
        !deprecation
    });
    for stmt in program.iter_mut() {
        if let StmtKind::NamespaceBlock { body, .. } = &mut stmt.kind {
            stripped |= strip_file_scope_deprecations(body);
        }
    }
    stripped
}

pub(super) fn first_file_scope_effect_call(program: &Program) -> Option<String> {
    /// Functions a class file may call at file scope: answers that do not change state, plus the
    /// alias and constant definitions that bind names, which are the same at boot as later.
    const INERT: &[&str] = &[
        "class_alias",
        "class_exists",
        "constant",
        "define",
        "defined",
        "dirname",
        "enum_exists",
        "extension_loaded",
        "function_exists",
        "interface_exists",
        "method_exists",
        "php_sapi_name",
        "phpversion",
        "trait_exists",
        "version_compare",
    ];

    /// The first effectful call inside one expression, closures included.
    fn expr_effect(expr: &Expr) -> Option<String> {
        let probe = Stmt::new(StmtKind::ExprStmt(expr.clone()), expr.span);
        let mut found = None;
        crate::parser::ast::visit::walk_nodes(std::slice::from_ref(&probe), &mut |node| {
            if found.is_some() {
                return;
            }
            let crate::parser::ast::visit::Node::Expr(expr) = node else {
                return;
            };
            found = match &expr.kind {
                ExprKind::FunctionCall { name, .. } => {
                    let function = name.as_canonical().trim_start_matches('\\').to_ascii_lowercase();
                    (!INERT.contains(&function.as_str())).then(|| format!("{function}()"))
                }
                ExprKind::MethodCall { method, .. }
                | ExprKind::NullsafeMethodCall { method, .. } => Some(format!("->{method}()")),
                ExprKind::StaticMethodCall { method, .. } => Some(format!("::{method}()")),
                ExprKind::NewObject { class_name, .. } => {
                    Some(format!("new {}", class_name.as_str()))
                }
                ExprKind::ClosureCall { .. } | ExprKind::ExprCall { .. } => {
                    Some("a callable".to_string())
                }
                _ => None,
            };
        });
        found
    }

    fn stmts_effect(stmts: &[Stmt]) -> Option<String> {
        stmts.iter().find_map(stmt_effect)
    }

    fn stmt_effect(stmt: &Stmt) -> Option<String> {
        match &stmt.kind {
            StmtKind::ClassDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. }
            | StmtKind::EnumDecl { .. }
            | StmtKind::FunctionDecl { .. }
            | StmtKind::NamespaceDecl { .. }
            | StmtKind::UseDecl { .. }
            | StmtKind::IncludeOnceMark { .. } => None,
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => stmts_effect(body),
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => expr_effect(condition)
                .or_else(|| stmts_effect(then_body))
                .or_else(|| {
                    elseif_clauses.iter().find_map(|(condition, body)| {
                        expr_effect(condition).or_else(|| stmts_effect(body))
                    })
                })
                .or_else(|| else_body.as_deref().and_then(stmts_effect)),
            StmtKind::Echo(_) => Some("echo".to_string()),
            StmtKind::ExprStmt(expr)
            | StmtKind::Assign { value: expr, .. }
            | StmtKind::ConstDecl { value: expr, .. } => expr_effect(expr),
            StmtKind::Return(value) => value.as_ref().and_then(expr_effect),
            _ => None,
        }
    }

    stmts_effect(program)
}

/// Returns whether `stmt` names class-like `fqn` from code that runs when the statement runs:
/// file-scope expressions and the bindings a declaration makes (`extends`, `implements`, trait
/// uses, constants, property defaults), but not the bodies of the functions and methods it
/// declares, which php only enters later -- if ever.
pub(super) fn stmt_references_executably(stmt: &Stmt, fqn: &str) -> bool {
    /// A copy of the statement whose function and method bodies are emptied.
    fn without_bodies(stmt: &Stmt) -> Option<Stmt> {
        let mut stmt = stmt.clone();
        match &mut stmt.kind {
            StmtKind::FunctionDecl { .. } => return None,
            StmtKind::ClassDecl { methods, .. }
            | StmtKind::InterfaceDecl { methods, .. }
            | StmtKind::TraitDecl { methods, .. }
            | StmtKind::EnumDecl { methods, .. } => {
                for method in methods.iter_mut() {
                    method.body.clear();
                }
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => {
                *body = body.iter().filter_map(without_bodies).collect();
            }
            _ => {}
        }
        Some(stmt)
    }
    let Some(stripped) = without_bodies(stmt) else {
        return false;
    };
    let mut refs = ReferenceSet::new(&HashMap::new());
    collect_refs_stmt(&stripped, &mut refs);
    let key = crate::names::php_symbol_key(fqn);
    refs.names
        .iter()
        .any(|name| crate::names::php_symbol_key(name) == key)
}

/// Collect the class-likes one freshly autoloaded file needs before its own top-level statements
/// can run: the binding dependencies of every declaration it makes, plus every reference point of
/// its executable file-scope statements, in source order.
///
/// Method bodies are deliberately left out. PHP binds a class without loading the classes its
/// methods merely mention, so following those here would drag the whole transitive vendor closure
/// in front of the file that asked for one class.
pub(super) fn collect_file_scope_dependencies(program: &Program) -> Vec<String> {
    let mut out = Vec::new();
    for stmt in program {
        collect_file_scope_dependencies_in_stmt(stmt, &mut out);
    }
    out
}

/// Recurse into one statement to collect the names its file-scope execution demands.
fn collect_file_scope_dependencies_in_stmt(stmt: &Stmt, out: &mut Vec<String>) {
    let push = |name: &crate::names::Name, out: &mut Vec<String>| {
        out.push(name.as_canonical().trim_start_matches('\\').to_string());
    };
    match &stmt.kind {
        StmtKind::ClassDecl {
            extends,
            implements,
            trait_uses,
            ..
        } => {
            if let Some(parent) = extends {
                push(parent, out);
            }
            for name in implements {
                push(name, out);
            }
            for trait_use in trait_uses {
                for name in &trait_use.trait_names {
                    push(name, out);
                }
            }
        }
        StmtKind::InterfaceDecl { extends, .. } => {
            for name in extends {
                push(name, out);
            }
        }
        StmtKind::TraitDecl { trait_uses, .. } => {
            for trait_use in trait_uses {
                for name in &trait_use.trait_names {
                    push(name, out);
                }
            }
        }
        StmtKind::EnumDecl {
            implements,
            trait_uses,
            ..
        } => {
            for name in implements {
                push(name, out);
            }
            for trait_use in trait_uses {
                for name in &trait_use.trait_names {
                    push(name, out);
                }
            }
        }
        StmtKind::PackedClassDecl { .. } | StmtKind::ClassLikeActivate { .. } => {}
        StmtKind::NamespaceBlock { body, .. } => {
            for inner in body {
                collect_file_scope_dependencies_in_stmt(inner, out);
            }
        }
        _ => {
            let mut refs = ReferenceSet::new(&HashMap::new());
            collect_refs_stmt(stmt, &mut refs);
            let mut names: Vec<String> = refs.names.into_iter().collect();
            names.sort();
            out.extend(names);
        }
    }
}

/// Collect all class reference points in the program, returning (statement index, FQN) pairs.
pub(super) fn collect_reference_points(program: &Program) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    // Autoload dependency insertion can place a parent declaration before the subclass that
    // supplies a class-string option consumed by the parent. Seed the flow facts from the whole
    // program so discovery does not depend on declaration order; the ordered pass below still
    // applies definite top-level overwrites at their observable reference points.
    let mut seed = ReferenceSet::new(&HashMap::new());
    for stmt in program {
        collect_refs_stmt(stmt, &mut seed);
    }
    let mut dynamic_class_defaults = seed.dynamic_class_defaults;
    for (stmt_idx, stmt) in program.iter().enumerate() {
        let mut refs = HashSet::new();
        collect_refs_with_definite_flow(stmt, &mut dynamic_class_defaults, &mut refs);
        let mut refs: Vec<String> = refs.into_iter().collect();
        refs.sort();
        out.extend(refs.into_iter().map(|fqn| (stmt_idx, fqn)));
    }
    out
}

/// Collects references while preserving sequential assignments through transparent AST wrappers.
fn collect_refs_with_definite_flow(
    stmt: &Stmt,
    defaults: &mut HashMap<DynamicClassTarget, HashSet<String>>,
    names: &mut HashSet<String>,
) {
    match &stmt.kind {
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::Synthetic(body) => {
            for inner in body {
                collect_refs_with_definite_flow(inner, defaults, names);
            }
        }
        _ => {
            // The statement's walk ends by handing its facts back, so it takes them over rather
            // than copying them: cloning the whole map per top-level statement was most of the
            // autoload discovery phase on a Symfony build.
            let mut refs = ReferenceSet::owning(std::mem::take(defaults));
            collect_refs_stmt(stmt, &mut refs);
            let local_names = refs.names;
            *defaults = refs.dynamic_class_defaults;
            names.extend(local_names);
        }
    }
}

/// Records top-level definite class-string assignments for later dynamic object construction.
fn update_dynamic_class_defaults(
    stmt: &Stmt,
    defaults: &mut HashMap<DynamicClassTarget, HashSet<String>>,
) {
    let (target, value) = match &stmt.kind {
        StmtKind::Assign { name, value } => (DynamicClassTarget::Variable(name.clone()), value),
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => {
            let Some(index) = dynamic_class_index(index) else {
                return;
            };
            (
                DynamicClassTarget::ArrayAccess {
                    array: Box::new(DynamicClassTarget::Variable(array.clone())),
                    index,
                },
                value,
            )
        }
        StmtKind::NestedArrayAssign { target, value } => {
            let Some(target) = dynamic_class_target(target) else {
                return;
            };
            (target, value)
        }
        _ => return,
    };

    update_dynamic_class_target_default(target, value, defaults);
}

/// Records possible class strings assigned to one stable dynamic class target.
fn update_dynamic_class_target_default(
    target: DynamicClassTarget,
    value: &Expr,
    defaults: &mut HashMap<DynamicClassTarget, HashSet<String>>,
) {
    if let Some(value) = literal_eval_return_value(value, defaults) {
        update_dynamic_class_target_default(target, &value, defaults);
        return;
    }
    if update_dynamic_class_array_defaults(&target, value, defaults) {
        return;
    }
    let mut candidates = HashSet::new();
    collect_possible_class_strings(value, defaults, &mut candidates);
    let preserves_previous = matches!(
        &value.kind,
        ExprKind::NullCoalesce { value: current, .. }
            if dynamic_class_target(current).as_ref() == Some(&target)
    );
    if preserves_previous {
        defaults.entry(target).or_default().extend(candidates);
    } else if candidates.is_empty() {
        defaults.remove(&target);
    } else {
        defaults.insert(target, candidates);
    }
}

/// Records per-key class-string defaults when a stable target receives a literal array.
///
/// A class name commonly crosses a dynamic boundary as `$classes[0]`. Recording only the
/// outer `$classes` local would lose that fact before `new $classes[0]` or
/// `class_exists($classes[0])` reaches the autoload walk.
fn update_dynamic_class_array_defaults(
    target: &DynamicClassTarget,
    value: &Expr,
    defaults: &mut HashMap<DynamicClassTarget, HashSet<String>>,
) -> bool {
    let entries = match &value.kind {
        ExprKind::ArrayLiteral(items) => items
            .iter()
            .enumerate()
            .map(|(index, value)| (DynamicClassIndex::Int(index as i64), value))
            .collect::<Vec<_>>(),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .filter_map(|(key, value)| Some((dynamic_class_index(key)?, value)))
            .collect::<Vec<_>>(),
        _ => return false,
    };
    defaults.remove(target);
    for (index, value) in entries {
        update_dynamic_class_target_default(
            DynamicClassTarget::ArrayAccess {
                array: Box::new(target.clone()),
                index,
            },
            value,
            defaults,
        );
    }
    true
}

/// Extracts the direct return expression from a statically known `eval` source.
///
/// This intentionally recognizes only an eval fragment consisting of one value return. More
/// involved runtime code remains opaque, while this shape safely preserves the same class-string
/// flow facts as a literal RHS without executing source during compilation.
fn literal_eval_return_value(
    expr: &Expr,
    defaults: &HashMap<DynamicClassTarget, HashSet<String>>,
) -> Option<Expr> {
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return None;
    };
    if name.as_canonical().trim_start_matches('\\') != "eval" || args.len() != 1 {
        return None;
    }
    let sources = literal_eval_source_candidates(&args[0], defaults);
    for source in sources {
        let source = format!("<?php\n{source}");
        let Ok(tokens) = crate::lexer::tokenize(&source) else {
            continue;
        };
        let Ok(program) = crate::parser::parse(&tokens) else {
            continue;
        };
        let [Stmt {
            kind: StmtKind::Return(Some(value)),
            ..
        }] = program.as_slice()
        else {
            continue;
        };
        return Some(value.clone());
    }
    None
}

/// Returns literal eval-source candidates flowing through a stable class-string local.
fn literal_eval_source_candidates(
    expr: &Expr,
    defaults: &HashMap<DynamicClassTarget, HashSet<String>>,
) -> Vec<String> {
    match &expr.kind {
        ExprKind::StringLiteral(source) => vec![source.clone()],
        ExprKind::Variable(_) | ExprKind::ArrayAccess { .. } => dynamic_class_target(expr)
            .and_then(|target| defaults.get(&target))
            .map(|sources| sources.iter().cloned().collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Collects class strings that a deterministic expression can produce from literals or tracked
/// stable l-values.
fn collect_possible_class_strings(
    expr: &Expr,
    defaults: &HashMap<DynamicClassTarget, HashSet<String>>,
    out: &mut HashSet<String>,
) {
    match &expr.kind {
        ExprKind::StringLiteral(name) => {
            let name = name.trim_start_matches('\\');
            if !name.is_empty() {
                out.insert(name.to_string());
            }
        }
        ExprKind::ClassConstant {
            receiver: StaticReceiver::Named(name),
        } => {
            let canonical = name.as_canonical();
            let canonical = canonical.trim_start_matches('\\');
            if !canonical.is_empty() {
                out.insert(canonical.to_string());
            }
        }
        ExprKind::Variable(_) | ExprKind::ArrayAccess { .. } => {
            if let Some(target) = dynamic_class_target(expr) {
                if let Some(candidates) = defaults.get(&target) {
                    out.extend(candidates.iter().cloned());
                }
            }
        }
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            collect_possible_class_strings(value, defaults, out);
            collect_possible_class_strings(default, defaults, out);
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            collect_possible_class_strings(then_expr, defaults, out);
            collect_possible_class_strings(else_expr, defaults, out);
        }
        _ => {}
    }
}

/// Converts a stable variable/literal-array l-value into a hashable flow key.
fn dynamic_class_target(expr: &Expr) -> Option<DynamicClassTarget> {
    match &expr.kind {
        ExprKind::Variable(name) => Some(DynamicClassTarget::Variable(name.clone())),
        ExprKind::ArrayAccess { array, index } => Some(DynamicClassTarget::ArrayAccess {
            array: Box::new(dynamic_class_target(array)?),
            index: dynamic_class_index(index)?,
        }),
        _ => None,
    }
}

/// Converts a literal string or integer array index into a stable flow key.
fn dynamic_class_index(expr: &Expr) -> Option<DynamicClassIndex> {
    match &expr.kind {
        ExprKind::StringLiteral(value) => Some(DynamicClassIndex::String(value.clone())),
        ExprKind::IntLiteral(value) => Some(DynamicClassIndex::Int(*value)),
        _ => None,
    }
}

/// Recurse into a statement to collect class references.
fn collect_refs_stmt(stmt: &Stmt, out: &mut ReferenceSet) {
    collect_attribute_groups(&stmt.attributes, out);
    match &stmt.kind {
        StmtKind::ClassDecl {
            extends,
            implements,
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            if let Some(parent) = extends {
                push_name(parent, out);
            }
            for iface in implements {
                push_name(iface, out);
            }
            for tu in trait_uses {
                collect_trait_use(tu, out);
            }
            for prop in properties {
                collect_property(prop, out);
            }
            for method in methods {
                collect_method(method, out);
            }
            collect_class_string_parameter_flows(methods, out);
            for constant in constants {
                collect_class_const(constant, out);
            }
        }
        StmtKind::InterfaceDecl {
            extends,
            properties,
            methods,
            constants,
            ..
        } => {
            for parent in extends {
                push_name(parent, out);
            }
            for prop in properties {
                collect_property(prop, out);
            }
            for method in methods {
                collect_method(method, out);
            }
            for constant in constants {
                collect_class_const(constant, out);
            }
        }
        StmtKind::TraitDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            for tu in trait_uses {
                collect_trait_use(tu, out);
            }
            for prop in properties {
                collect_property(prop, out);
            }
            for method in methods {
                collect_method(method, out);
            }
            for constant in constants {
                collect_class_const(constant, out);
            }
        }
        StmtKind::EnumDecl {
            backing_type: _,
            cases,
            implements,
            trait_uses,
            methods,
            constants,
            ..
        } => {
            for interface in implements {
                push_name(interface, out);
            }
            for trait_use in trait_uses {
                collect_trait_use(trait_use, out);
            }
            for case in cases {
                collect_attribute_groups(&case.attributes, out);
                if let Some(value) = &case.value {
                    collect_refs_expr(value, out);
                }
            }
            for method in methods {
                collect_method(method, out);
            }
            for constant in constants {
                collect_class_const(constant, out);
            }
        }
        StmtKind::PackedClassDecl { fields, .. } => {
            for field in fields {
                collect_type_expr(&field.type_expr, out);
            }
        }
        StmtKind::FunctionDecl {
            params,
            param_attributes,
            variadic_type,
            return_type,
            body,
            ..
        } => {
            for groups in param_attributes {
                collect_attribute_groups(groups, out);
            }
            collect_callable_signature(params, variadic_type.as_ref(), return_type.as_ref(), out);
            for s in body {
                collect_refs_stmt(s, out);
            }
        }
        StmtKind::NamespaceBlock { body, .. } => {
            for inner in body {
                collect_refs_stmt(inner, out);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                collect_refs_stmt(s, out);
            }
            if let Some(body) = else_body {
                for s in body {
                    collect_refs_stmt(s, out);
                }
            }
        }
        StmtKind::Assign { value, .. } => collect_refs_expr(value, out),
        StmtKind::RefAssign { .. } => {}
        StmtKind::ExprStmt(e) => collect_refs_expr(e, out),
        StmtKind::Return(Some(e)) => collect_refs_expr(e, out),
        StmtKind::Echo(e) => collect_refs_expr(e, out),
        StmtKind::Include { path, .. } => collect_refs_expr(path, out),
        StmtKind::IncludeOnceGuard { body, .. } => {
            for s in body {
                collect_refs_stmt(s, out);
            }
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_refs_expr(condition, out);
            for s in then_body {
                collect_refs_stmt(s, out);
            }
            for (cond, body) in elseif_clauses {
                collect_refs_expr(cond, out);
                for s in body {
                    collect_refs_stmt(s, out);
                }
            }
            if let Some(body) = else_body {
                for s in body {
                    collect_refs_stmt(s, out);
                }
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            collect_refs_expr(condition, out);
            for s in body {
                collect_refs_stmt(s, out);
            }
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(s) = init {
                collect_refs_stmt(s, out);
            }
            if let Some(c) = condition {
                collect_refs_expr(c, out);
            }
            if let Some(s) = update {
                collect_refs_stmt(s, out);
            }
            for s in body {
                collect_refs_stmt(s, out);
            }
        }
        StmtKind::Foreach {
            array, body, ..
        } => {
            collect_refs_expr(array, out);
            for s in body {
                collect_refs_stmt(s, out);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            for s in try_body {
                collect_refs_stmt(s, out);
            }
            for catch in catches {
                collect_refs_catch(catch, out);
            }
            if let Some(f) = finally_body {
                for s in f {
                    collect_refs_stmt(s, out);
                }
            }
        }
        StmtKind::Switch { subject, cases, default } => {
            collect_refs_expr(subject, out);
            for (values, body) in cases {
                for v in values {
                    collect_refs_expr(v, out);
                }
                for s in body {
                    collect_refs_stmt(s, out);
                }
            }
            if let Some(default_body) = default {
                for s in default_body {
                    collect_refs_stmt(s, out);
                }
            }
        }
        StmtKind::ConstDecl { value, .. } | StmtKind::StaticVar { init: value, .. } => {
            collect_refs_expr(value, out);
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyRefAssign { object, source: value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            collect_refs_expr(object, out);
            collect_refs_expr(value, out);
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            collect_refs_expr(object, out);
            collect_refs_expr(index, out);
            collect_refs_expr(value, out);
        }
        StmtKind::StaticPropertyAssign {
            receiver, value, ..
        }
        | StmtKind::StaticPropertyArrayPush {
            receiver, value, ..
        } => {
            collect_static_receiver(receiver, out);
            collect_refs_expr(value, out);
        }
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            index,
            value,
            ..
        } => {
            collect_static_receiver(receiver, out);
            collect_refs_expr(index, out);
            collect_refs_expr(value, out);
        }
        StmtKind::StaticPropertyElementRefAssign {
            receiver,
            index,
            source,
            ..
        } => {
            collect_static_receiver(receiver, out);
            collect_refs_expr(index, out);
            collect_refs_expr(source, out);
        }
        StmtKind::ArrayAssign { value, index, .. } => {
            collect_refs_expr(value, out);
            collect_refs_expr(index, out);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            collect_refs_expr(target, out);
            collect_refs_expr(value, out);
        }
        StmtKind::ArrayPush { value, .. } => collect_refs_expr(value, out),
        StmtKind::ListUnpack { value, .. } => collect_refs_expr(value, out),
        StmtKind::TypedAssign { value, .. } => collect_refs_expr(value, out),
        StmtKind::Throw(e) => collect_refs_expr(e, out),
        StmtKind::Synthetic(stmts) => {
            for s in stmts {
                collect_refs_stmt(s, out);
            }
        }
        _ => {}
    }
    update_dynamic_class_defaults(stmt, &mut out.dynamic_class_defaults);
}

/// Collect class references from a catch clause.
fn collect_refs_catch(catch: &CatchClause, out: &mut ReferenceSet) {
    if type_positions_autoload() {
        for exception_type in &catch.exception_types {
            push_name(exception_type, out);
        }
    }
    for s in &catch.body {
        collect_refs_stmt(s, out);
    }
}

/// Collect class references from a class method declaration.
fn collect_method(method: &ClassMethod, out: &mut ReferenceSet) {
    collect_attribute_groups(&method.attributes, out);
    for groups in &method.param_attributes {
        collect_attribute_groups(groups, out);
    }
    collect_callable_signature(
        &method.params,
        method.variadic_type.as_ref(),
        method.return_type.as_ref(),
        out,
    );
    for s in &method.body {
        collect_refs_stmt(s, out);
    }
}

/// Collect class references from a class property declaration.
fn collect_property(prop: &ClassProperty, out: &mut ReferenceSet) {
    collect_attribute_groups(&prop.attributes, out);
    if let Some(d) = &prop.default {
        collect_refs_expr(d, out);
    }
}

/// Collect class references from a class constant declaration.
fn collect_class_const(constant: &ClassConst, out: &mut ReferenceSet) {
    collect_attribute_groups(&constant.attributes, out);
    collect_refs_expr(&constant.value, out);
}

/// Collects parameter default expressions without treating declaration type names as autoload demands.
///
/// PHP defers resolving named types in function-like signatures until an operation needs their
/// metadata. Loading a type merely because it occurs in a callback annotation would execute
/// optional autoload sources and change the program's observable top-level behavior.
fn collect_callable_signature(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    _variadic_type: Option<&TypeExpr>,
    _return_type: Option<&TypeExpr>,
    out: &mut ReferenceSet,
) {
    for (_, _, default, _) in params {
        if let Some(default) = default {
            collect_refs_expr(default, out);
        }
    }
}

/// Collects attribute class names and references nested in their argument expressions.
fn collect_attribute_groups(groups: &[AttributeGroup], out: &mut ReferenceSet) {
    for group in groups {
        for attribute in &group.attributes {
            push_name(&attribute.name, out);
            for argument in &attribute.args {
                collect_refs_expr(argument, out);
                collect_attribute_class_constant_refs(argument, out);
            }
        }
    }
}

/// Collects class-string constants retained as attribute metadata dependencies.
///
/// Ordinary `Type::class` expressions do not autoload the named type in PHP, so the general
/// expression walker deliberately ignores them. Attribute metadata is different: reflection may
/// later instantiate or inspect the recorded class string, and AOT must have discovered an
/// existing declaration before reachability can keep it. Nested constant-expression arrays and
/// named arguments are traversed recursively.
fn collect_attribute_class_constant_refs(expr: &Expr, out: &mut ReferenceSet) {
    match &expr.kind {
        ExprKind::ClassConstant {
            receiver: StaticReceiver::Named(name),
        } => push_name(name, out),
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_attribute_class_constant_refs(item, out);
            }
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                collect_attribute_class_constant_refs(key, out);
                collect_attribute_class_constant_refs(value, out);
            }
        }
        ExprKind::NamedArg { value, .. }
        | ExprKind::Negate(value)
        | ExprKind::Not(value)
        | ExprKind::BitNot(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Spread(value)
        | ExprKind::Cast { expr: value, .. } => {
            collect_attribute_class_constant_refs(value, out);
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_attribute_class_constant_refs(left, out);
            collect_attribute_class_constant_refs(right, out);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_attribute_class_constant_refs(condition, out);
            collect_attribute_class_constant_refs(then_expr, out);
            collect_attribute_class_constant_refs(else_expr, out);
        }
        ExprKind::ShortTernary { value, default }
        | ExprKind::NullCoalesce { value, default } => {
            collect_attribute_class_constant_refs(value, out);
            collect_attribute_class_constant_refs(default, out);
        }
        _ => {}
    }
}

/// Collect class references from a trait use declaration.
fn collect_trait_use(trait_use: &TraitUse, out: &mut ReferenceSet) {
    for name in &trait_use.trait_names {
        push_name(name, out);
    }
    for adaptation in &trait_use.adaptations {
        match adaptation {
            TraitAdaptation::Alias { trait_name, .. } => {
                if let Some(name) = trait_name {
                    push_name(name, out);
                }
            }
            TraitAdaptation::InsteadOf {
                trait_name,
                instead_of,
                ..
            } => {
                if let Some(name) = trait_name {
                    push_name(name, out);
                }
                for name in instead_of {
                    push_name(name, out);
                }
            }
        }
    }
}

/// php autoloads nothing for a name in a type, an `instanceof` or a `catch`, so a closed world
/// does not compile a class only such positions name: declaring it would also make
/// `class_exists($name, false)` true where php answers false. A class the program uses reaches
/// the world through that use (`new`, a static call, `extends`/`implements`).
fn type_positions_autoload() -> bool {
    !crate::closed_world::is_enabled()
}

/// Collect class references from a type expression.
fn collect_type_expr(ty: &TypeExpr, out: &mut ReferenceSet) {
    match ty {
        TypeExpr::Named(name) => {
            if type_positions_autoload() {
                push_name(name, out)
            }
        }
        TypeExpr::Array(inner) => collect_type_expr(inner, out),
        // Both halves: a class named only in `array<string, Foo>` still has to be discovered,
        // or autoloading never pulls it in and the program fails at the first use.
        TypeExpr::AssocArray { key, value } => {
            collect_type_expr(key, out);
            collect_type_expr(value, out);
        }
        TypeExpr::Nullable(inner) => collect_type_expr(inner, out),
        TypeExpr::Union(parts) | TypeExpr::Intersection(parts) => {
            for p in parts {
                collect_type_expr(p, out);
            }
        }
        TypeExpr::Buffer(inner) => collect_type_expr(inner, out),
        TypeExpr::Ptr(Some(name)) => {
            let canonical = name.as_canonical();
            let leaf = crate::names::pointer_target_leaf(&canonical).trim_start_matches('\\');
            if !leaf.is_empty() { out.insert(leaf.to_string()); }
        }
        // The TEMPLATE is the file to load: `Box<User>` is declared in whatever file declares
        // `class Box`, and the instantiated class does not exist until after autoloading. Its
        // arguments name classes with files of their own, so both halves are collected — the
        // same reason `array<string, Foo>` collects both.
        TypeExpr::GenericClass { name, args } => {
            push_name(name, out);
            for arg in args {
                collect_type_expr(arg, out);
            }
        }
        _ => {}
    }
}

/// Recurse into an expression to collect class references, including compile-time
/// autoload demands from `class_exists`/`interface_exists`/etc. with literal arguments.
fn collect_refs_expr(expr: &Expr, out: &mut ReferenceSet) {
    match &expr.kind {
        ExprKind::NewObject { class_name, args } => {
            push_name(class_name, out);
            if reflection_constructor_autoloads_class_target(class_name) {
                collect_class_like_name_argument(args.first(), out);
            }
            for a in args {
                collect_refs_expr(a, out);
            }
        }
        // `new Box<User>(...)` demands the file declaring `Box` and the file declaring `User`,
        // and its constructor arguments can name classes of their own.
        ExprKind::NewGeneric { class_type, args } => {
            collect_type_expr(class_type, out);
            for a in args {
                collect_refs_expr(a, out);
            }
        }
        ExprKind::InstanceOf { value, target } => {
            collect_refs_expr(value, out);
            match target {
                crate::parser::ast::InstanceOfTarget::Name(name) => {
                    if type_positions_autoload() {
                        push_name(name, out)
                    }
                }
                // `instanceof Box<User>` demands both files, exactly as the type position does.
                crate::parser::ast::InstanceOfTarget::Generic(class_type) => {
                    collect_type_expr(class_type, out)
                }
                crate::parser::ast::InstanceOfTarget::Expr(inner) => collect_refs_expr(inner, out),
            }
        }
        ExprKind::NewDynamic { name_expr, args } => {
            collect_dynamic_class_target_candidates(name_expr, out);
            collect_refs_expr(name_expr, out);
            for arg in args {
                collect_refs_expr(arg, out);
            }
        }
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class,
            required_parent,
            args,
        } => {
            collect_refs_expr(class_name, out);
            push_name(fallback_class, out);
            push_name(required_parent, out);
            for arg in args {
                collect_refs_expr(arg, out);
            }
        }
        // `Foo::class` is NOT a load. PHP resolves it lexically, from the name alone, and never
        // consults the autoloader — `Foo::class` is legal for a class that does not exist at all.
        // Collecting it here was tried and measured: it pulled `DoctrineDbalAdapter` into the
        // build off a `sprintf()` argument in `PdoAdapter`, and that file `use`s
        // `Doctrine\DBAL\Connection` from a package this app does not install, so the build
        // then failed on a class PHP would never have loaded. The plain `index.php` entry went
        // from 7 errors to 30 that way, against one error saved on the widest preload. The four
        // existence functions stay the exception, in their own arm above: handing `Foo::class` to
        // `class_exists()` is what runs the loader.
        //
        // A closed-world build is the exception. It has no loader to fall back on, and a class
        // named only as `Foo::class` is exactly what a framework instantiates from a string later:
        // `Route::compile()` calls `$class::compile()` on its `'compiler_class' =>
        // RouteCompiler::class` option, and a container rebuild checks every service class its
        // config files name. There the name joins the world as a PROBE demand: compiled, but
        // still "not loaded" to `class_exists($n, false)` until something uses it, as in php.
        ExprKind::ClassConstant {
            receiver: StaticReceiver::Named(name),
        } if crate::closed_world::is_enabled() => {
            let cleaned = name.as_canonical().trim_start_matches('\\').to_string();
            if !cleaned.is_empty() {
                out.insert_probe(cleaned);
            }
        }
        ExprKind::ClassConstant { .. } => {}
        ExprKind::ScopedConstantAccess { receiver, .. }
        | ExprKind::StaticPropertyAccess { receiver, .. } => {
            collect_static_receiver(receiver, out);
        }
        ExprKind::DynamicScopedConstantAccess { receiver, .. } => {
            collect_refs_expr(receiver, out);
        }
        ExprKind::StaticMethodCall {
            receiver, args, ..
        } => {
            collect_static_receiver(receiver, out);
            for a in args {
                collect_refs_expr(a, out);
            }
        }
        ExprKind::Cast { expr: inner, .. } => collect_refs_expr(inner, out),
        ExprKind::PtrCast { expr: inner, .. } => collect_refs_expr(inner, out),
        ExprKind::BinaryOp { left, right, .. } => {
            collect_refs_expr(left, out);
            collect_refs_expr(right, out);
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner) => collect_refs_expr(inner, out),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            collect_refs_expr(value, out);
            collect_refs_expr(default, out);
        }
        ExprKind::Pipe { value, callable } => {
            collect_refs_expr(value, out);
            collect_refs_expr(callable, out);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_refs_expr(condition, out);
            collect_refs_expr(then_expr, out);
            collect_refs_expr(else_expr, out);
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_refs_expr(subject, out);
            for (patterns, value) in arms {
                for p in patterns {
                    collect_refs_expr(p, out);
                }
                collect_refs_expr(value, out);
            }
            if let Some(d) = default {
                collect_refs_expr(d, out);
            }
        }
        ExprKind::Assignment { target, value, prelude, .. } => {
            collect_refs_expr(target, out);
            collect_refs_expr(value, out);
            for s in prelude {
                collect_refs_stmt(s, out);
            }
            if let Some(target) = dynamic_class_target(target) {
                update_dynamic_class_target_default(
                    target,
                    value,
                    &mut out.dynamic_class_defaults,
                );
            }
        }
        ExprKind::FunctionCall { name, args } => {
            // Detect compile-time demands for a literal class name. The
            // autoload pass picks these up like any other class reference.
            let canonical = name.as_canonical();
            let trimmed = canonical.trim_start_matches('\\');
            match trimmed {
                "spl_autoload_call" | "spl_autoload" => {
                    push_literal_fqn(args.first(), out);
                }
                "class_exists" | "interface_exists" | "trait_exists" | "enum_exists" => {
                    // The autoload-controlling second arg only triggers when
                    // omitted or when it is a literal truthy value. A dynamic
                    // expression must not be guessed as true at AOT time.
                    let triggers_autoload = match args.get(1).map(|arg| &arg.kind) {
                        None => true,
                        Some(ExprKind::BoolLiteral(b)) => *b,
                        Some(ExprKind::IntLiteral(n)) => *n != 0,
                        Some(_) => false,
                    };
                    if !triggers_autoload {
                        match args.first().map(|arg| &arg.kind) {
                            Some(ExprKind::StringLiteral(name)) => {
                                let cleaned = name.trim_start_matches('\\').to_string();
                                if !cleaned.is_empty() {
                                    out.insert_probe_noload(cleaned);
                                }
                            }
                            Some(_) => {
                                if let Some(cleaned) = args.first().and_then(class_constant_fqn) {
                                    out.insert_probe_noload(cleaned);
                                }
                            }
                            None => {}
                        }
                    }
                    if triggers_autoload {
                        push_probe_literal_fqn(args.first(), out);
                        // `Foo::class` does not autoload on its own — the general walker ignores
                        // it for that reason — but handing it to one of these four functions is
                        // what runs the loader. `symfony/string`'s `AsciiSlugger.php` guards its
                        // whole file with `interface_exists(LocaleAwareInterface::class)`, and
                        // without this the interface never entered the closed world and the
                        // compiled program threw its "package is not installed" LogicException on
                        // boot.
                        push_probe_class_constant_fqn(args.first(), out);
                        if args
                            .first()
                            .is_some_and(|arg| !matches!(arg.kind, ExprKind::StringLiteral(_)))
                        {
                            collect_dynamic_class_target_candidates(
                                args.first().expect("checked above"),
                                out,
                            );
                        }
                    }
                }
                "property_exists" | "method_exists" => {
                    // php autoloads the class named by the first argument before answering. The
                    // native member tables only know classes the compilation saw, so an unseen
                    // class answered `false` (Symfony's FrameworkExtension asks
                    // `property_exists(ValidatorExtension::class, 'violationMapper')` and dropped
                    // `form.violation_mapper`). Pulling the class in as a PROBE-ONLY demand keeps
                    // the build closed-world: no interpreter. It is a USE, not a probe-only demand:
                    // a probe-only class stays "deferred" until a loading probe runs, and the
                    // native member tables answer `false` for a deferred class.
                    push_literal_fqn(args.first(), out);
                    if let Some(name) = args.first().and_then(class_constant_fqn) {
                        out.insert(name);
                    }
                }
                "class_alias" if class_alias_autoloads_target(args) => {
                    collect_class_like_name_argument(args.first(), out);
                }
                _ => {}
            }
            for a in args {
                collect_refs_expr(a, out);
            }
        }
        ExprKind::ClosureCall { args, .. } => {
            for a in args {
                collect_refs_expr(a, out);
            }
        }
        ExprKind::ExprCall { callee, args } => {
            collect_refs_expr(callee, out);
            for a in args {
                collect_refs_expr(a, out);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_refs_expr(array, out);
            collect_refs_expr(index, out);
        }
        ExprKind::ArrayLiteral(items) => {
            if items.len() == 2 {
                collect_dynamic_class_target_candidates(&items[0], out);
                push_callable_array_class(items, out);
            }
            for i in items {
                collect_refs_expr(i, out);
            }
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (k, v) in pairs {
                collect_refs_expr(k, out);
                collect_refs_expr(v, out);
            }
        }
        ExprKind::ArrayLiteralMixed(entries) => {
            for entry in entries {
                for expr in entry.exprs() {
                    collect_refs_expr(expr, out);
                }
            }
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. }
        | ExprKind::ObjectClassName { object } => collect_refs_expr(object, out),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            collect_refs_expr(object, out);
            for a in args {
                collect_refs_expr(a, out);
            }
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            collect_refs_expr(object, out);
            collect_refs_expr(method, out);
            for a in args {
                collect_refs_expr(a, out);
            }
        }
        ExprKind::NewScopedObject { receiver, args } => {
            collect_static_receiver(receiver, out);
            for a in args {
                collect_refs_expr(a, out);
            }
        }
        ExprKind::NamedArg { value, .. } => collect_refs_expr(value, out),
        ExprKind::Closure {
            params,
            variadic_type,
            return_type,
            body,
            ..
        } => {
            collect_callable_signature(params, variadic_type.as_ref(), return_type.as_ref(), out);
            for s in body {
                collect_refs_stmt(s, out);
            }
        }
        ExprKind::FirstClassCallable(target) => collect_callable_target(target, out),
        ExprKind::BufferNew { element_type, len } => {
            collect_type_expr(element_type, out);
            collect_refs_expr(len, out);
        }
        ExprKind::Yield { key, value } => {
            if let Some(k) = key {
                collect_refs_expr(k, out);
            }
            if let Some(v) = value {
                collect_refs_expr(v, out);
            }
        }
        ExprKind::YieldFrom(inner) => collect_refs_expr(inner, out),
        _ => {}
    }
}

/// Follows a class string from a call in a class into the method parameter that loads it.
///
/// `class_exists($class)` and `new $class()` on a PARAMETER name no class by themselves; the
/// class arrives at the call. symfony/framework-bundle's `FrameworkBundle::build()` calls
/// `$this->addCompilerPassIfExists($container, RoutingControllerPass::class)` a dozen times, and
/// the private helper does `if (class_exists($class)) { $container->addCompilerPass(new $class()); }`.
/// php loads each pass there; the walker saw only `$class` and left them all out of the world,
/// so a closed-world console could not build its container. For every method of the class whose
/// parameter reaches such a sink, the literal or `X::class` argument at that position in a
/// `$this->m(...)`, `self::m(...)` or `static::m(...)` call of the same class is collected the way
/// the sink collects a literal: a probe for the existence functions, a use for `new`.
fn collect_class_string_parameter_flows(methods: &[ClassMethod], out: &mut ReferenceSet) {
    use crate::parser::ast::visit::{walk_nodes, Node};
    let mut sinks: HashMap<String, Vec<(usize, bool)>> = HashMap::new();
    for method in methods {
        for (index, (param, ..)) in method.params.iter().enumerate() {
            if let Some(probe_only) = parameter_class_string_sink(&method.body, param) {
                sinks
                    .entry(method.name.to_ascii_lowercase())
                    .or_default()
                    .push((index, probe_only));
            }
        }
    }
    if sinks.is_empty() {
        return;
    }
    for method in methods {
        walk_nodes(&method.body, &mut |node| {
            let Node::Expr(expr) = node else {
                return;
            };
            let (callee, args) = match &expr.kind {
                ExprKind::MethodCall { object, method, args }
                    if matches!(object.kind, ExprKind::This) =>
                {
                    (method, args)
                }
                ExprKind::StaticMethodCall {
                    receiver: StaticReceiver::Self_ | StaticReceiver::Static,
                    method,
                    args,
                } => (method, args),
                _ => return,
            };
            let Some(positions) = sinks.get(&callee.to_ascii_lowercase()) else {
                return;
            };
            for (index, probe_only) in positions {
                let arg = args.get(*index);
                if *probe_only {
                    push_probe_literal_fqn(arg, out);
                    push_probe_class_constant_fqn(arg, out);
                } else {
                    push_literal_fqn(arg, out);
                    if let Some(name) = arg.and_then(class_constant_fqn) {
                        out.insert(name);
                    }
                }
            }
        });
    }
}

/// Whether `param` is handed to an existence probe (`Some(true)`) or to `new` (`Some(false)`) in
/// `body`, and nothing else is assigned to it first.
fn parameter_class_string_sink(body: &[Stmt], param: &str) -> Option<bool> {
    use crate::parser::ast::visit::{walk_nodes, Node};
    let is_param = |expr: &Expr| matches!(&expr.kind, ExprKind::Variable(name) if name == param);
    let mut sink: Option<bool> = None;
    let mut reassigned = false;
    walk_nodes(body, &mut |node| match node {
        Node::Stmt(stmt) => {
            if matches!(&stmt.kind, StmtKind::Assign { name, .. } if name == param) {
                reassigned = true;
            }
        }
        Node::Expr(expr) => match &expr.kind {
            ExprKind::FunctionCall { name, args } => {
                let canonical = name.as_canonical();
                let function = canonical.trim_start_matches('\\');
                let probe = matches!(
                    function,
                    "class_exists" | "interface_exists" | "trait_exists" | "enum_exists"
                );
                let autoloads = match args.get(1).map(|arg| &arg.kind) {
                    None => true,
                    Some(ExprKind::BoolLiteral(value)) => *value,
                    Some(_) => false,
                };
                if probe && autoloads && args.first().is_some_and(is_param) {
                    sink.get_or_insert(true);
                }
            }
            ExprKind::NewDynamic { name_expr, .. } if is_param(name_expr) => sink = Some(false),
            _ => {}
        },
    });
    if reassigned {
        None
    } else {
        sink
    }
}

/// Adds known class-string candidates used as a dynamic class target in the current expression.
fn collect_dynamic_class_target_candidates(expr: &Expr, out: &mut ReferenceSet) {
    let Some(target) = dynamic_class_target(expr) else {
        return;
    };
    let Some(candidates) = out.dynamic_class_defaults.get(&target) else {
        return;
    };
    let candidates = candidates.iter().cloned().collect::<Vec<_>>();
    for candidate in candidates {
        out.insert(candidate);
    }
}

/// Extracts the class named by a `[Foo::class, 'method']` callable array.
///
/// Distinct from the blanket `::class` rule this walker deliberately does NOT have (see the
/// `ClassConstant` arm): a bare `Foo::class` is a string PHP resolves lexically and may hand to
/// anything — `sprintf()` included — whereas THIS shape is built to be called, and calling it is
/// what runs the autoloader. The second element must be a string literal, so a two-element data
/// pair that merely happens to start with a `::class` is not mistaken for one.
///
/// Twig's `EscaperExtension` needs it: `$defaultStrategy = [FileExtensionEscapingStrategy::class,
/// 'guess'];` was the program's only reference to that class, and the file is right there in the
/// package — the build reported `Undefined class: Twig\FileExtensionEscapingStrategy` for a class
/// it could have read off disk.
fn push_callable_array_class(items: &[Expr], out: &mut ReferenceSet) {
    let [class_const, method] = items else {
        return;
    };
    if !matches!(method.kind, ExprKind::StringLiteral(_)) {
        return;
    }
    if let ExprKind::ClassConstant {
        receiver: StaticReceiver::Named(name),
    } = &class_const.kind
    {
        push_name(name, out);
    }
}

/// Collect a class reference from a static receiver (::scope).
fn collect_static_receiver(receiver: &StaticReceiver, out: &mut ReferenceSet) {
    match receiver {
        StaticReceiver::Named(name) => push_name(name, out),
        // `Box<User>::of()` demands the file declaring `Box` and the file declaring `User`.
        StaticReceiver::Generic(class_type) => collect_type_expr(class_type, out),
        StaticReceiver::Self_ | StaticReceiver::Static | StaticReceiver::Parent => {}
    }
}

/// Collect a class reference from a first-class callable target.
fn collect_callable_target(target: &CallableTarget, out: &mut ReferenceSet) {
    match target {
        CallableTarget::StaticMethod { receiver, .. } => collect_static_receiver(receiver, out),
        CallableTarget::Method { object, .. } => collect_refs_expr(object, out),
        CallableTarget::Function(_) => {}
    }
}

/// Normalize a name to its canonical FQN (strip leading `\`), then insert it into `out` if non-empty.
fn push_name(name: &crate::names::Name, out: &mut ReferenceSet) {
    let canonical = name.as_canonical();
    let trimmed = canonical.trim_start_matches('\\');
    if !trimmed.is_empty() {
        out.insert(trimmed.to_string());
    }
}

/// Records a `class_exists()`-family literal argument as a PROBE-ONLY demand.
///
/// Kept separate from `push_literal_fqn` because the two spellings are separate facts: a string
/// literal names a class the call may autoload, and so does `::class`, but `::class` ANYWHERE
/// ELSE does not autoload and must keep being ignored by the general walker.
///
/// The name still enters the closed world -- the probe has to be answerable -- but it is not
/// counted as a use, so [`probe_only_class_names`] can report it as a class php would not have
/// loaded.
fn push_probe_literal_fqn(arg: Option<&crate::parser::ast::Expr>, out: &mut ReferenceSet) {
    let Some(arg) = arg else { return };
    let ExprKind::StringLiteral(name) = &arg.kind else {
        return;
    };
    let cleaned = name.trim_start_matches('\\').to_string();
    if !cleaned.is_empty() {
        out.insert_probe(cleaned);
    }
}

/// The class named by a literal `X::class` argument, without a leading backslash.
fn class_constant_fqn(arg: &crate::parser::ast::Expr) -> Option<String> {
    let ExprKind::ClassConstant {
        receiver: StaticReceiver::Named(name),
    } = &arg.kind
    else {
        return None;
    };
    let cleaned = name.as_canonical().trim_start_matches('\\').to_string();
    (!cleaned.is_empty()).then_some(cleaned)
}

/// Records a `class_exists(X::class)`-style argument as a PROBE-ONLY demand.
fn push_probe_class_constant_fqn(arg: Option<&crate::parser::ast::Expr>, out: &mut ReferenceSet) {
    let Some(arg) = arg else { return };
    if let ExprKind::ClassConstant {
        receiver: StaticReceiver::Named(name),
    } = &arg.kind
    {
        let cleaned = name.as_canonical().trim_start_matches('\\').to_string();
        if !cleaned.is_empty() {
            out.insert_probe(cleaned);
        }
    }
}

/// Extract a literal string FQN from an expression argument (used for `class_exists` etc.).
/// Inserts the cleaned FQN into `out` if the argument is a string literal.
fn push_literal_fqn(arg: Option<&crate::parser::ast::Expr>, out: &mut ReferenceSet) {
    let Some(arg) = arg else { return };
    let ExprKind::StringLiteral(name) = &arg.kind else {
        return;
    };
    let cleaned = name.trim_start_matches('\\').to_string();
    if !cleaned.is_empty() {
        out.insert(cleaned);
    }
}

/// Returns whether `class_alias()` is allowed to autoload its original class argument.
fn class_alias_autoloads_target(args: &[Expr]) -> bool {
    match args.get(2).map(|arg| &arg.kind) {
        None => true,
        Some(ExprKind::BoolLiteral(value)) => *value,
        Some(ExprKind::IntLiteral(value)) => *value != 0,
        Some(_) => false,
    }
}

/// Returns whether a reflection constructor autoloads a class-like first argument in PHP.
fn reflection_constructor_autoloads_class_target(class_name: &crate::names::Name) -> bool {
    let class_name = class_name
        .as_canonical()
        .trim_start_matches('\\')
        .to_ascii_lowercase();
    matches!(
        class_name.as_str(),
        "reflectionclass"
            | "reflectionenum"
            | "reflectionclassconstant"
            | "reflectionmethod"
            | "reflectionproperty"
    )
}

/// Records a statically named class-like argument when a PHP API autoloads it.
fn collect_class_like_name_argument(arg: Option<&Expr>, out: &mut ReferenceSet) {
    let Some(arg) = arg else {
        return;
    };
    let arg = match &arg.kind {
        ExprKind::NamedArg { value, .. } => value.as_ref(),
        _ => arg,
    };
    match &arg.kind {
        ExprKind::StringLiteral(name) => {
            let name = name.trim_start_matches('\\');
            if !name.is_empty() {
                out.insert(name.to_string());
            }
        }
        ExprKind::ClassConstant {
            receiver: StaticReceiver::Named(name),
        } => push_name(name, out),
        _ => {}
    }
}
