//! Purpose:
//! Compiles a construct the checker refused into the run-time fault php raises for it, so the
//! class file around it stays compiled instead of being handed to the interpreter.
//!
//! Called from:
//! - `crate::pipeline::compile()`, in the type-check loop, before a refused class file is deferred
//!   (`autoload::deferral`).
//!
//! Key details:
//! - php binds a member or a class name when the expression EXECUTES. `if (method_exists(X::class,
//!   'm')) { X::m(); }` and `$walker->walkWhenClauseExpression($this)` in a method nothing calls
//!   are loadable programs, and vendor code is full of them. The checker refuses the body all the
//!   same, because a compiled call needs a target, and the whole file used to go to the runtime
//!   autoloader for it (symfony/demo: 87 files; Laravel's artisan: 190).
//! - Only an error the checker TAGGED (`CompileError::runtime_fault`) is a candidate: the checker
//!   is the one that knows whether the refusal is php's own run-time error or a gap of its own
//!   typing. This module adds what the checker cannot see, which is whether the compiled world
//!   holds the class in full: a class with a subclass in a deferred file, a builtin, or a name
//!   some file could still supply makes the fault uncertain, and the file is deferred as before.
//! - An instance member is rewritten into a helper that takes the receiver as `mixed` and
//!   resolves the member by NAME when it runs (`property_exists`/`method_exists`, then the
//!   runtime's dynamic read or call): a subclass the walker never saw, `__get`/`__call`, or an
//!   interpreted object all keep php's behaviour, and the miss raises php's own Warning or Error.
//!   A static member on a fixed class, and a class nothing declares, become the `throw` php
//!   would raise there; a `static::` receiver is late-bound and is left alone.
//! - A rewritten node keeps its source span, so a stack frame or a `getLine()` names the user's
//!   line; the nodes built around it carry no location.
//! - Each demotion is reported as a `note:` beside the deferral warnings, so a build log lists
//!   what was compiled as a fault the way it lists what was not compiled at all.

use crate::fast_hash::{FastMap as HashMap, FastSet as HashSet};
use std::path::{Path, PathBuf};

use elephc_builtin_contract::ClassKind;

use crate::errors::{CompileError, RuntimeFault};
use crate::names::php_symbol_key;
use crate::parser::ast::visit::{walk_nodes_mut, NodeMut};
use crate::parser::ast::{BinOp, Expr, ExprKind, Program, StaticReceiver, Stmt, StmtKind, TypeExpr};
use crate::span::Span;
use crate::synthetic_class::{
    e_array, e_binop, e_call, e_concat_all, e_dyn_prop, e_int, e_new_fq, e_not, e_null, e_str,
    e_var, fq_name, function, php_semantics_declarations, s_if, s_return, s_throw, t_array,
    t_mixed,
};

/// Reserved name of the helper an undefined-property read is compiled through.
pub(crate) const PROPERTY_READ_HELPER: &str = "__elephc_runtime_property_read";

/// Reserved name of the helper an undefined-method call is compiled through.
pub(crate) const METHOD_CALL_HELPER: &str = "__elephc_runtime_method_call";

/// Reachability group the helpers are recorded under.
pub(crate) const PRELUDE_GROUP: &str = "runtime_faults";

/// What the pipeline knows about the compiled world that the checker does not.
pub struct World<'a> {
    /// Every class-like name to the file that declares it (autoloaded and preloaded).
    pub class_files: &'a HashMap<String, String>,
    /// The files the pipeline may hand to the runtime autoloader.
    pub deferrable: &'a HashSet<PathBuf>,
    /// The files it already has.
    pub deferred: &'a HashSet<PathBuf>,
    /// Each class-like of the program as it was BEFORE any deferral removed declarations, with
    /// the names it extends, implements or uses (`deferral::class_parent_edges`).
    pub class_parents: &'a [(String, Vec<String>)],
    /// Answers whether the project's autoloader could supply a class the walker never loaded.
    pub autoload_lookup: &'a dyn Fn(&str) -> bool,
}

/// One construct compiled as a fault.
#[derive(Debug, Clone)]
pub struct Demotion {
    /// The class file the construct is in.
    pub path: String,
    /// The file the construct was written in, when a trait spliced it in from another file.
    pub source: Option<String>,
    /// Where the construct is.
    pub span: Span,
    /// The checker's message for it, which with `span` identifies the error it answers.
    pub message: String,
    /// What it now does when it runs.
    pub outcome: String,
}

impl Demotion {
    /// The `note:` line for the build log.
    pub fn note(&self) -> String {
        let source = self
            .source
            .as_deref()
            .map(|source| format!("{source}:"))
            .unwrap_or_default();
        format!(
            "autoload: '{}' is compiled; {source}{}:{}: {}",
            self.path, self.span.line, self.span.col, self.outcome
        )
    }
}

/// The helper functions the rewrites call, built once per program that needs them.
///
/// Both take the receiver as `mixed`, which is what makes them faithful: a `mixed` receiver is
/// resolved by the runtime against the object's actual class, compiled or interpreted, and
/// `__get`/`__call` take part the way they do in php. The miss is spelled here rather than left
/// to the runtime's dynamic read, which answers null without a Warning, and to its dynamic call,
/// which dies with an uncatchable "callable array did not resolve" instead of php's `Error`.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![property_read_helper(), method_call_helper()])
}

/// `__elephc_runtime_property_read(mixed $object, string $property, bool $nullsafe): mixed`.
fn property_read_helper() -> Stmt {
    let warning = |message: Expr| {
        // 512 is `E_USER_WARNING`, which php displays as `Warning:` like the engine's own.
        s_if(
            e_call("trigger_error", vec![message, e_int(512)]),
            vec![s_return(e_null())],
            vec![],
            Some(vec![s_return(e_null())]),
        )
    };
    function(PROPERTY_READ_HELPER)
        .param("object", t_mixed())
        .param("property", TypeExpr::Str)
        .param("nullsafe", TypeExpr::Bool)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(
                    e_binop(e_var("object"), BinOp::StrictEq, e_null()),
                    BinOp::And,
                    e_var("nullsafe"),
                ),
                vec![s_return(e_null())],
                vec![],
                None,
            ),
            s_if(
                e_not(e_call("is_object", vec![e_var("object")])),
                vec![warning(e_concat_all(vec![
                    e_str("Attempt to read property \""),
                    e_var("property"),
                    e_str("\" on "),
                    e_call("get_debug_type", vec![e_var("object")]),
                ]))],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_call("property_exists", vec![e_var("object"), e_var("property")]),
                    BinOp::Or,
                    e_call("method_exists", vec![e_var("object"), e_str("__get")]),
                ),
                vec![s_return(e_dyn_prop(e_var("object"), e_var("property")))],
                vec![],
                None,
            ),
            warning(e_concat_all(vec![
                e_str("Undefined property: "),
                e_call("get_class", vec![e_var("object")]),
                e_str("::$"),
                e_var("property"),
            ])),
        ])
        .build()
}

/// `__elephc_runtime_method_call(mixed $object, string $method, array $arguments, bool $nullsafe): mixed`.
fn method_call_helper() -> Stmt {
    function(METHOD_CALL_HELPER)
        .param("object", t_mixed())
        .param("method", TypeExpr::Str)
        .param("arguments", t_array())
        .param("nullsafe", TypeExpr::Bool)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(
                    e_binop(e_var("object"), BinOp::StrictEq, e_null()),
                    BinOp::And,
                    e_var("nullsafe"),
                ),
                vec![s_return(e_null())],
                vec![],
                None,
            ),
            s_if(
                e_not(e_call("is_object", vec![e_var("object")])),
                vec![s_throw(e_new_fq(
                    "Error",
                    vec![e_concat_all(vec![
                        e_str("Call to a member function "),
                        e_var("method"),
                        e_str("() on "),
                        e_call("get_debug_type", vec![e_var("object")]),
                    ])],
                ))],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_not(e_call("method_exists", vec![e_var("object"), e_var("method")])),
                    BinOp::And,
                    e_not(e_call("method_exists", vec![e_var("object"), e_str("__call")])),
                ),
                vec![s_throw(e_new_fq(
                    "Error",
                    vec![e_concat_all(vec![
                        e_str("Call to undefined method "),
                        e_call("get_class", vec![e_var("object")]),
                        e_str("::"),
                        e_var("method"),
                        e_str("()"),
                    ])],
                ))],
                vec![],
                None,
            ),
            // The parser's own spelling of `$object->$method(...$arguments)`.
            s_return(e_call(
                "call_user_func_array",
                vec![
                    e_array(vec![e_var("object"), e_var("method")]),
                    e_var("arguments"),
                ],
            )),
        ])
        .build()
}

/// Rewrites every tagged error of `error` (and its related ones) whose declaration lives in a
/// deferrable, not yet deferred, class file, and returns what was rewritten.
///
/// `attempted` remembers each `(span, message)` tried, so an error the rewrite did not make go
/// away is not tried again: the next round defers the file as before.
pub fn demote(
    program: &mut Program,
    error: &CompileError,
    world: &World<'_>,
    attempted: &mut HashSet<(Span, String)>,
) -> Vec<Demotion> {
    let declared = DeclaredClassLikes::collect(program);
    let open = open_classes(world);
    let mut demotions = Vec::new();
    for error in std::iter::once(error).chain(error.related.iter()) {
        let Some(fault) = error.runtime_fault.as_ref() else {
            continue;
        };
        let Some(declaration) = error.declaration.as_deref() else {
            continue;
        };
        let Some(path) = class_file(world.class_files, declaration) else {
            continue;
        };
        let canonical_path = canonical(Path::new(path));
        if !world.deferrable.contains(&canonical_path) || world.deferred.contains(&canonical_path) {
            continue;
        }
        // Keyed by the declaration too: a trait's construct has ONE span, and each class using the
        // trait carries its own copy to rewrite. laravel/pao's `TestResultParsable` is used by two
        // `Starter` classes, and the second copy's error looked like the first one coming back.
        if !attempted.insert((error.span, format!("{}\0{}", error.message, declaration))) {
            continue;
        }
        let Some(plan) = plan(fault, world, &declared, &open, error.span) else {
            continue;
        };
        let outcome = rewrite(program, declaration, error.span, &plan)
            .or_else(|| rewrite_in_trait_copy(program, declaration, error.span, &plan));
        let Some(outcome) = outcome else {
            continue;
        };
        let source = Span::source_paths_snapshot()
            .get(&error.span.source_id())
            .filter(|source| canonical(Path::new(source.as_str())) != canonical_path)
            .cloned();
        demotions.push(Demotion {
            path: path.clone(),
            source,
            span: error.span,
            message: error.message.clone(),
            outcome,
        });
    }
    demotions
}

/// Which rewrite a fault gets.
enum Plan {
    /// `$o->m(...)` through `METHOD_CALL_HELPER`.
    MethodCall { method: String },
    /// `$o->p` through `PROPERTY_READ_HELPER`.
    PropertyRead { property: String },
    /// `X::m(...)`, `X::$p`, `X::C`, `new X`, and a too-few-arguments `$o->m(...)`:
    /// `throw new \<error_class>(message)` in place.
    Throw { shape: StaticShape, message: String, error_class: &'static str },
}

/// Whether `PDO::C` names a constant of a PDO driver the build leaves out.
///
/// php defines `PDO::SQLSRV_*`, `PDO::OCI_*` and the like only while the driver's extension is
/// loaded; without it the read raises "Undefined constant". The PDO prelude declares exactly the
/// drivers the build enables (`OptionalDrivers`), so for a disabled one the fault is certain, and
/// Doctrine DBAL's `PDO\SQLSrv\Statement` keeps compiling for the line it never reaches.
fn is_disabled_pdo_driver_constant(class: &str, constant: &str) -> bool {
    if !class.trim_start_matches('\\').eq_ignore_ascii_case("PDO") {
        return false;
    }
    let drivers = crate::pdo_prelude::OptionalDrivers::from_build_environment();
    [
        ("SQLSRV_", drivers.sqlsrv),
        ("OCI_", drivers.oci),
        ("FB_", drivers.firebird),
        ("IBM_", drivers.ibm),
        ("CUBRID_", drivers.cubrid),
    ]
    .iter()
    .any(|(prefix, enabled)| constant.starts_with(prefix) && !enabled)
}

/// The static construct a `Plan::Throw` replaces.
enum StaticShape {
    Method { class: String, method: String },
    /// `late_bound` admits a `static::` receiver too: the class has no descendant anywhere,
    /// so the called class can only be the class itself.
    Property { class: String, property: String, late_bound: bool },
    Constant { class: String, constant: String },
    /// Any of the above, or `new`, naming `class`.
    AnyNaming { class: String },
    /// `$o->m(...)` (never `?->`, whose null receiver makes no call) whose receiver and
    /// arguments have no effect: php evaluates them before it raises, so none may be dropped.
    InstanceCall { method: String },
}

/// Decides whether the compiled world vouches for `fault`, and how to compile it.
fn plan(
    fault: &RuntimeFault,
    world: &World<'_>,
    declared: &DeclaredClassLikes,
    open: &HashSet<String>,
    span: Span,
) -> Option<Plan> {
    // The compiled world holds a user class in full when the program declares it from source
    // (a prelude's model of a builtin is partial by design), no builtin stands behind the name
    // or above it, and no deferred file declares a descendant that could supply the member.
    //
    // The ancestry matters as much as the name: `DatePoint extends \DateTimeImmutable` calls
    // `$this->__unserialize()`, which php finds on the builtin parent and the checker's model of
    // that parent lacks. A builtin interface above the class contributes only constants, so it
    // only makes a CONSTANT fault uncertain (`\DateTimeInterface::ATOM`).
    let ancestry_held = |class: &str, constant_fault: bool| {
        let key = php_symbol_key(class.trim_start_matches('\\'));
        declared.from_source(&key)
            && !is_builtin_class(class)
            && ancestors(world.class_parents, &key).iter().all(|ancestor| {
                match builtin_kind(ancestor) {
                    // A builtin interface is never declared in the program; it is held by
                    // the catalog (`PhpParser\NodeAbstract implements \JsonSerializable`).
                    Some(ClassKind::Interface) => !constant_fault,
                    Some(_) => false,
                    None => declared.contains(ancestor),
                }
            })
    };
    let held_in_full = |class: &str| {
        let key = php_symbol_key(class.trim_start_matches('\\'));
        ancestry_held(class, false) && !open.contains(&key)
    };
    let fixed_class = |class: &str, constant_fault: bool| ancestry_held(class, constant_fault);
    match fault {
        RuntimeFault::UndefinedMethod { class, method } => held_in_full(class).then(|| Plan::MethodCall {
            method: method.clone(),
        }),
        RuntimeFault::UndefinedProperty { class, property } => {
            held_in_full(class).then(|| Plan::PropertyRead {
                property: property.clone(),
            })
        }
        RuntimeFault::UndefinedStaticMethod { class, method } => fixed_class(class, false).then(|| Plan::Throw {
            shape: StaticShape::Method {
                class: class.clone(),
                method: method.clone(),
            },
            error_class: "Error",
            message: format!(
                "Call to undefined method {}::{}()",
                class.trim_start_matches('\\'),
                method
            ),
        }),
        RuntimeFault::UndefinedStaticProperty { class, property } => {
            fixed_class(class, false).then(|| Plan::Throw {
                shape: StaticShape::Property {
                    class: class.clone(),
                    property: property.clone(),
                    // Laravel's `HasFactory::newFactory()` reads `static::$factory` behind
                    // `isset(static::$factory)`, in the copy `App\Models\User` gets: a leaf model
                    // that declares none, so the guarded read is php's Error if it ever runs.
                    late_bound: held_in_full(class) && is_leaf_class(world.class_parents, class),
                },
                error_class: "Error",
                message: format!(
                    "Access to undeclared static property {}::${}",
                    class.trim_start_matches('\\'),
                    property
                ),
            })
        }
        RuntimeFault::UndefinedClassConstant { class, constant } => {
            (fixed_class(class, true) || is_disabled_pdo_driver_constant(class, constant)).then(|| Plan::Throw {
                shape: StaticShape::Constant {
                    class: class.clone(),
                    constant: constant.clone(),
                },
                error_class: "Error",
                message: format!(
                    "Undefined constant {}::{}",
                    class.trim_start_matches('\\'),
                    constant
                ),
            })
        }
        // A descendant may redeclare the method with fewer required parameters, so only a class
        // the compiled world holds in full makes the count certain.
        RuntimeFault::TooFewArguments { class, method, passed, required, exactly } => {
            held_in_full(class).then(|| {
                let file = Span::source_paths_snapshot()
                    .get(&span.source_id())
                    .cloned()
                    .unwrap_or_default();
                Plan::Throw {
                    shape: StaticShape::InstanceCall { method: method.clone() },
                    message: format!(
                        "Too few arguments to function {}::{}(), {} passed in {} on line {} and {} {} expected",
                        class.trim_start_matches('\\'),
                        method,
                        passed,
                        file,
                        span.line,
                        if *exactly { "exactly" } else { "at least" },
                        required
                    ),
                    error_class: "ArgumentCountError",
                }
            })
        }
        RuntimeFault::UnknownClass { class } => {
            // "Not found" is only php's answer when no file can supply the class: one a
            // deferred file declares, one the walker dropped with its parent, or one the
            // autoloader's index could still load all exist at run time.
            let key = php_symbol_key(class.trim_start_matches('\\'));
            let nowhere = !declared.contains(&key)
                && class_file(world.class_files, class).is_none()
                && !(world.autoload_lookup)(class)
                && !is_builtin_class(class);
            nowhere.then(|| Plan::Throw {
                shape: StaticShape::AnyNaming {
                    class: class.clone(),
                },
                error_class: "Error",
                message: format!("Class \"{}\" not found", class.trim_start_matches('\\')),
            })
        }
    }
}

/// Is `name` a class-like the compiler models itself?
fn is_builtin_class(name: &str) -> bool {
    builtin_kind(name).is_some()
}

/// What kind of class-like the builtin catalog declares under `name`, if any.
fn builtin_kind(name: &str) -> Option<ClassKind> {
    crate::types::builtin_classes::lookup_profile_class(name).map(|contract| contract.kind)
}

/// Whether no class-like the program knows extends, implements or uses `class`.
fn is_leaf_class(class_parents: &[(String, Vec<String>)], class: &str) -> bool {
    let key = php_symbol_key(class.trim_start_matches('\\'));
    !class_parents.iter().any(|(_, parents)| {
        parents
            .iter()
            .any(|parent| php_symbol_key(parent.trim_start_matches('\\')) == key)
    })
}

/// Every class-like above `key` (extended, implemented or used), transitively, by symbol key.
fn ancestors(class_parents: &[(String, Vec<String>)], key: &str) -> Vec<String> {
    let parents: HashMap<String, &Vec<String>> = class_parents
        .iter()
        .map(|(class, parents)| (php_symbol_key(class.trim_start_matches('\\')), parents))
        .collect();
    let mut seen: HashSet<String> = HashSet::default();
    let mut out = Vec::new();
    let mut pending = vec![key.to_string()];
    while let Some(class) = pending.pop() {
        let Some(direct) = parents.get(&class) else {
            continue;
        };
        for parent in direct.iter() {
            let parent = php_symbol_key(parent.trim_start_matches('\\'));
            if seen.insert(parent.clone()) {
                out.push(parent.clone());
                pending.push(parent);
            }
        }
    }
    out
}

/// Every ancestor-or-self of a class a deferred file declares: a receiver of such a class may
/// hold, at run time, an object of the deferred subclass, which may declare the member.
fn open_classes(world: &World<'_>) -> HashSet<String> {
    let parents: HashMap<String, &Vec<String>> = world
        .class_parents
        .iter()
        .map(|(class, parents)| (php_symbol_key(class.trim_start_matches('\\')), parents))
        .collect();
    let mut open = HashSet::default();
    let mut pending: Vec<String> = world
        .class_parents
        .iter()
        .filter(|(class, _)| {
            class_file(world.class_files, class)
                .is_some_and(|path| world.deferred.contains(&canonical(Path::new(path))))
        })
        .map(|(class, _)| php_symbol_key(class.trim_start_matches('\\')))
        .collect();
    while let Some(class) = pending.pop() {
        if !open.insert(class.clone()) {
            continue;
        }
        if let Some(parents) = parents.get(&class) {
            pending.extend(
                parents
                    .iter()
                    .map(|parent| php_symbol_key(parent.trim_start_matches('\\'))),
            );
        }
    }
    open
}

/// The class-likes the program declares, by symbol key.
struct DeclaredClassLikes {
    /// Whether each one was written in source (a prelude's declaration carries no location).
    from_source: HashMap<String, bool>,
}

impl DeclaredClassLikes {
    fn collect(program: &Program) -> Self {
        let mut from_source = HashMap::default();
        fn walk(stmts: &[Stmt], out: &mut HashMap<String, bool>) {
            for stmt in stmts {
                match &stmt.kind {
                    StmtKind::ClassDecl { name, .. }
                    | StmtKind::InterfaceDecl { name, .. }
                    | StmtKind::TraitDecl { name, .. }
                    | StmtKind::EnumDecl { name, .. } => {
                        out.insert(
                            php_symbol_key(name.trim_start_matches('\\')),
                            stmt.span.is_from_source(),
                        );
                    }
                    StmtKind::NamespaceBlock { body, .. }
                    | StmtKind::Synthetic(body)
                    | StmtKind::IncludeOnceGuard { body, .. } => walk(body, out),
                    StmtKind::If {
                        then_body,
                        elseif_clauses,
                        else_body,
                        ..
                    } => {
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
        walk(program, &mut from_source);
        Self { from_source }
    }

    fn contains(&self, key: &str) -> bool {
        self.from_source.contains_key(key)
    }

    fn from_source(&self, key: &str) -> bool {
        self.from_source.get(key).copied().unwrap_or(false)
    }
}

/// Applies `plan` to the node at `span` inside the class-like named `declaration`, and returns
/// what the node now does. `None` when no node of the expected shape sits at that span.
fn rewrite(program: &mut Program, declaration: &str, span: Span, plan: &Plan) -> Option<String> {
    let key = php_symbol_key(declaration.trim_start_matches('\\'));
    let methods = find_class_like_methods(program, &key)?;
    let mut outcome = None;
    for method in methods.iter_mut() {
        if outcome.is_some() {
            break;
        }
        walk_nodes_mut(&mut method.body, &mut |node| {
            if outcome.is_some() {
                return;
            }
            match node {
                NodeMut::Stmt(stmt) => {
                    let Plan::Throw { shape, message, error_class } = plan else {
                        return;
                    };
                    // A `throw` left INSIDE a typed position (`strtoupper(X::C)`) lowers to a
                    // placeholder the backend cannot type, so the throw is hoisted to the
                    // statement when nothing with an effect runs before the construct: php
                    // observes the same, since the statement's value never arrives.
                    let hoist = match &mut stmt.kind {
                        StmtKind::Return(Some(value))
                        | StmtKind::ExprStmt(value)
                        | StmtKind::Echo(value) => {
                            throw_reaches_first(value, span, shape).then_some(Hoist::Statement)
                        }
                        // An assignment keeps binding its target, or every later read of it
                        // would be refused as undefined; the throw becomes its value.
                        StmtKind::Assign { value, .. }
                        | StmtKind::TypedAssign { value, .. }
                        | StmtKind::PropertyAssign { value, .. }
                        | StmtKind::StaticPropertyAssign { value, .. } => {
                            (value.span != span && throw_reaches_first(value, span, shape))
                                .then_some(Hoist::Value)
                        }
                        _ => None,
                    };
                    match hoist {
                        Some(Hoist::Statement) => {
                            let throwable = error_object(error_class, message, span);
                            *stmt = Stmt {
                                kind: StmtKind::Throw(throwable),
                                span: stmt.span,
                                source_mode: stmt.source_mode,
                                strict_types: stmt.strict_types,
                                attributes: Vec::new(),
                            };
                            outcome = Some(throw_outcome(error_class, message));
                        }
                        Some(Hoist::Value) => {
                            let Some(value) = statement_value_mut(stmt) else {
                                return;
                            };
                            let throwable = error_object(error_class, message, span);
                            *value = Expr::new(ExprKind::Throw(Box::new(throwable)), span);
                            outcome = Some(throw_outcome(error_class, message));
                        }
                        None => {}
                    }
                }
                NodeMut::Expr(expr) => {
                    if expr.span != span {
                        return;
                    }
                    outcome = rewrite_expr(expr, plan);
                }
            }
        });
    }
    outcome
}

/// Finds the method bodies of the class-like named `key`, through the file-level wrappers.
/// Rewrites a fault that sits in a TRAIT `declaration` uses, in a copy of the trait's method made
/// the declaration's own.
///
/// Laravel's `ResolvesDumpSource::isCompiledViewFile()` reads `$this->compiledViewPath`, which its
/// `Exceptions\Renderer\Frame` user does not declare and its dumper users do (`protected`).
/// Rewriting the trait itself would send those users' reads through the by-name helper too, which
/// runs outside the class and is refused the protected property. A class's own method takes
/// precedence over a trait's in php, so the copy changes nothing but this class's fault.
fn rewrite_in_trait_copy(
    program: &mut Program,
    declaration: &str,
    span: Span,
    plan: &Plan,
) -> Option<String> {
    let key = php_symbol_key(declaration.trim_start_matches('\\'));
    for trait_name in used_traits(program, declaration) {
        let trait_key = php_symbol_key(&trait_name);
        let Some(candidates) = find_class_like_methods(program, &trait_key).map(|m| m.clone()) else {
            continue;
        };
        for method in candidates {
            let own = find_class_like_methods(program, &key)?;
            if own.iter().any(|existing| existing.name.eq_ignore_ascii_case(&method.name)) {
                continue;
            }
            own.push(method);
            if let Some(outcome) = rewrite(program, declaration, span, plan) {
                return Some(outcome);
            }
            find_class_like_methods(program, &key)?.pop();
        }
    }
    None
}

/// Every trait `declaration` uses, directly or through the traits it uses, in use order.
fn used_traits(program: &Program, declaration: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut pending = vec![declaration.trim_start_matches('\\').to_string()];
    while let Some(name) = pending.pop() {
        let key = php_symbol_key(&name);
        crate::parser::ast::visit::walk_stmts(program, &mut |stmt| {
            let (StmtKind::ClassDecl { name, trait_uses, .. }
            | StmtKind::TraitDecl { name, trait_uses, .. }
            | StmtKind::EnumDecl { name, trait_uses, .. }) = &stmt.kind
            else {
                return;
            };
            if php_symbol_key(name.trim_start_matches('\\')) != key {
                return;
            }
            for used in trait_uses.iter().flat_map(|uses| &uses.trait_names) {
                let used = used.as_canonical().trim_start_matches('\\').to_string();
                if !found.iter().any(|seen| php_symbol_key(seen) == php_symbol_key(&used)) {
                    found.push(used.clone());
                    pending.push(used);
                }
            }
        });
    }
    found
}

fn find_class_like_methods<'a>(
    stmts: &'a mut [Stmt],
    key: &str,
) -> Option<&'a mut Vec<crate::parser::ast::ClassMethod>> {
    for stmt in stmts.iter_mut() {
        match &mut stmt.kind {
            StmtKind::ClassDecl { name, methods, .. }
            | StmtKind::InterfaceDecl { name, methods, .. }
            | StmtKind::TraitDecl { name, methods, .. }
            | StmtKind::EnumDecl { name, methods, .. } => {
                if php_symbol_key(name.trim_start_matches('\\')) == key {
                    return Some(methods);
                }
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => {
                if let Some(found) = find_class_like_methods(body, key) {
                    return Some(found);
                }
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                if let Some(found) = find_class_like_methods(then_body, key) {
                    return Some(found);
                }
                for (_, body) in elseif_clauses.iter_mut() {
                    if let Some(found) = find_class_like_methods(body, key) {
                        return Some(found);
                    }
                }
                if let Some(body) = else_body {
                    if let Some(found) = find_class_like_methods(body, key) {
                        return Some(found);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Where a hoisted throw goes.
enum Hoist {
    /// The statement becomes `throw new \Error(...)`.
    Statement,
    /// The statement keeps its shape and the throw becomes its value.
    Value,
}

/// The value expression of an assignment-like statement.
fn statement_value_mut(stmt: &mut Stmt) -> Option<&mut Expr> {
    match &mut stmt.kind {
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::PropertyAssign { value, .. }
        | StmtKind::StaticPropertyAssign { value, .. } => Some(value),
        _ => None,
    }
}

/// Is the construct at `span` the first thing with an effect that evaluating `root` runs?
///
/// True when the construct is `root` itself or sits below it along unconditional operands,
/// with every operand evaluated before it free of effects. Then throwing in place of the whole
/// of `root` changes nothing php can observe: no call, assignment or construction ran before
/// the fault, and nothing after it runs at all.
fn throw_reaches_first(root: &Expr, span: Span, shape: &StaticShape) -> bool {
    fn search(expr: &Expr, span: Span, shape: &StaticShape) -> Option<bool> {
        if expr.span == span && matches_static_shape(expr, shape) {
            return Some(true);
        }
        // Operands in php's evaluation order; a conditional operand (a ternary arm, the right
        // side of `&&`) is not listed, so a construct inside one is never hoisted past it.
        let operands: Vec<&Expr> = match &expr.kind {
            ExprKind::FunctionCall { args, .. }
            | ExprKind::NewObject { args, .. }
            | ExprKind::NewScopedObject { args, .. }
            | ExprKind::StaticMethodCall { args, .. }
            | ExprKind::ArrayLiteral(args) => args.iter().collect(),
            ExprKind::MethodCall { object, args, .. } => {
                std::iter::once(object.as_ref()).chain(args.iter()).collect()
            }
            ExprKind::BinaryOp { left, op, right } if !matches!(op, BinOp::And | BinOp::Or) => {
                vec![left, right]
            }
            ExprKind::ArrayLiteralAssoc(pairs) => {
                pairs.iter().flat_map(|(key, value)| [key, value]).collect()
            }
            ExprKind::ArrayAccess { array, index } => vec![array, index],
            ExprKind::PropertyAccess { object, .. } => vec![object],
            ExprKind::Negate(inner)
            | ExprKind::Not(inner)
            | ExprKind::BitNot(inner)
            | ExprKind::Cast { expr: inner, .. }
            | ExprKind::NamedArg { value: inner, .. }
            | ExprKind::Spread(inner)
            | ExprKind::Print(inner)
            | ExprKind::Clone(inner) => vec![inner],
            _ => return None,
        };
        for (index, operand) in operands.iter().enumerate() {
            if let Some(reaches) = search(operand, span, shape) {
                return Some(reaches && operands[..index].iter().all(|earlier| is_pure(earlier)));
            }
        }
        None
    }
    search(root, span, shape).unwrap_or(false)
}

/// Can evaluating `expr` run no user code and change no state?
fn is_pure(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::Variable(_)
        | ExprKind::This
        | ExprKind::ConstRef(_)
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::MagicConstant(_) => true,
        ExprKind::PropertyAccess { object, .. } => is_pure(object),
        ExprKind::ArrayAccess { array, index } => is_pure(array) && is_pure(index),
        ExprKind::BinaryOp { left, right, .. } => is_pure(left) && is_pure(right),
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::NamedArg { value: inner, .. } => is_pure(inner),
        ExprKind::ArrayLiteral(items) => items.iter().all(is_pure),
        ExprKind::ArrayLiteralAssoc(pairs) => {
            pairs.iter().all(|(key, value)| is_pure(key) && is_pure(value))
        }
        _ => false,
    }
}

/// Rewrites one expression in place and returns what it now does.
fn rewrite_expr(expr: &mut Expr, plan: &Plan) -> Option<String> {
    match plan {
        Plan::MethodCall { method } => {
            let (object, args, nullsafe) = match &expr.kind {
                ExprKind::MethodCall { object, method: name, args }
                    if name.eq_ignore_ascii_case(method) =>
                {
                    (object, args, false)
                }
                ExprKind::NullsafeMethodCall { object, method: name, args }
                    if name.eq_ignore_ascii_case(method) =>
                {
                    (object, args, true)
                }
                _ => return None,
            };
            // A named or spread argument has no place in the helper's positional array.
            if args.iter().any(|arg| {
                matches!(arg.kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_))
            }) {
                return None;
            }
            let call = Expr::new(
                ExprKind::FunctionCall {
                    name: fq_name(METHOD_CALL_HELPER),
                    args: vec![
                        object.as_ref().clone(),
                        e_str(method),
                        e_array(args.clone()),
                        bool_literal(nullsafe),
                    ],
                },
                expr.span,
            );
            *expr = call;
            Some(format!(
                "the call to `{}{}()` is resolved by name when it runs, and php's `Call to \
                 undefined method` Error is thrown if the run-time object lacks it",
                if nullsafe { "?->" } else { "->" },
                method
            ))
        }
        Plan::PropertyRead { property } => {
            let (object, nullsafe) = match &expr.kind {
                ExprKind::PropertyAccess { object, property: name } if name == property => {
                    (object, false)
                }
                ExprKind::NullsafePropertyAccess { object, property: name } if name == property => {
                    (object, true)
                }
                _ => return None,
            };
            let read = Expr::new(
                ExprKind::FunctionCall {
                    name: fq_name(PROPERTY_READ_HELPER),
                    args: vec![
                        object.as_ref().clone(),
                        e_str(property),
                        bool_literal(nullsafe),
                    ],
                },
                expr.span,
            );
            *expr = read;
            Some(format!(
                "the read of `{}{}` is resolved by name when it runs, and php's `Undefined \
                 property` Warning is raised (and null read) if the run-time object lacks it",
                if nullsafe { "?->" } else { "->" },
                property
            ))
        }
        Plan::Throw { shape, message, error_class } => {
            // `static::FIREWALL_OPTION` in a trait method (Symfony's `FirewallAwareTrait`, guarded
            // by `defined(static::class.'::FIREWALL_OPTION')`): the called class decides, so the
            // read becomes the run-time lookup `constant(static::class . '::C')`, which answers a
            // descendant's constant and raises php's own "Undefined constant" Error otherwise.
            if let (
                StaticShape::Constant { constant, .. },
                ExprKind::ScopedConstantAccess { receiver: StaticReceiver::Static, name },
            ) = (shape, &expr.kind)
            {
                if name != constant {
                    return None;
                }
                let called_class =
                    Expr::new(ExprKind::ClassConstant { receiver: StaticReceiver::Static }, expr.span);
                let qualified = e_binop(called_class, BinOp::Concat, e_str(&format!("::{constant}")));
                *expr = Expr::new(
                    ExprKind::FunctionCall { name: fq_name("constant"), args: vec![qualified] },
                    expr.span,
                );
                return Some(format!(
                    "`static::{constant}` is looked up on the called class when it runs, and \
                     php's `Undefined constant` Error is thrown if that class lacks it"
                ));
            }
            if !matches_static_shape(expr, shape) {
                return None;
            }
            let throwable = error_object(error_class, message, expr.span);
            *expr = Expr::new(ExprKind::Throw(Box::new(throwable)), expr.span);
            Some(throw_outcome(error_class, message))
        }
    }
}

/// Does `expr` spell the static construct the fault was raised for?
///
/// A `static::` receiver is excluded everywhere: it is bound to the called class when the
/// expression runs, so the fault the checker judged against the lexical class is not certain.
fn matches_static_shape(expr: &Expr, shape: &StaticShape) -> bool {
    let same_class = |receiver: &StaticReceiver, class: &str| match receiver {
        // A generic receiver is instantiated into an ordinary named one before type checking;
        // a template has no class to reach through.
        StaticReceiver::Generic(_) => unreachable!(
            "StaticReceiver::Generic must be instantiated by generics::classes"
        ),
        StaticReceiver::Named(name) => same_symbol(name.as_str(), class),
        StaticReceiver::Self_ | StaticReceiver::Parent => true,
        StaticReceiver::Static => false,
    };
    match (shape, &expr.kind) {
        (
            StaticShape::Method { class, method },
            ExprKind::StaticMethodCall { receiver, method: name, .. },
        ) => same_class(receiver, class) && name.eq_ignore_ascii_case(method),
        (
            StaticShape::Property { class, property, late_bound },
            ExprKind::StaticPropertyAccess { receiver, property: name },
        ) => {
            name == property
                && (same_class(receiver, class)
                    || (*late_bound && matches!(receiver, StaticReceiver::Static)))
        }
        (
            StaticShape::Constant { class, constant },
            ExprKind::ScopedConstantAccess { receiver, name },
        ) => same_class(receiver, class) && name == constant,
        (StaticShape::AnyNaming { class }, ExprKind::NewObject { class_name, .. }) => {
            same_symbol(class_name.as_str(), class)
        }
        (
            StaticShape::AnyNaming { class },
            ExprKind::StaticMethodCall { receiver, .. }
            | ExprKind::StaticPropertyAccess { receiver, .. }
            | ExprKind::ScopedConstantAccess { receiver, .. },
        ) => matches!(receiver, StaticReceiver::Named(name) if same_symbol(name.as_str(), class)),
        (StaticShape::InstanceCall { method }, ExprKind::MethodCall { object, method: name, args }) => {
            name.eq_ignore_ascii_case(method) && is_pure(object) && args.iter().all(is_pure)
        }
        _ => false,
    }
}

fn same_symbol(left: &str, right: &str) -> bool {
    php_symbol_key(left.trim_start_matches('\\')) == php_symbol_key(right.trim_start_matches('\\'))
}

/// `new \<class>(message)` at `span`, so the Error reports the user's line.
fn error_object(class: &str, message: &str, span: Span) -> Expr {
    let mut object = e_new_fq(class, vec![e_str(message)]);
    object.span = span;
    object
}

fn throw_outcome(class: &str, message: &str) -> String {
    format!("php's {class} `{message}` is thrown when it runs")
}

fn bool_literal(value: bool) -> Expr {
    Expr::new(ExprKind::BoolLiteral(value), Span::dummy())
}

/// Looks up the file that declares class-like `name` (see `deferral::class_file`).
fn class_file<'a>(class_files: &'a HashMap<String, String>, name: &str) -> Option<&'a String> {
    super::deferral::class_file(class_files, name)
}

fn canonical(path: &Path) -> PathBuf {
    crate::source::canonical(&path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).unwrap();
        crate::parser::parse(&tokens).unwrap()
    }

    /// Finds the span of the first expression of a kind in a program.
    fn span_of(program: &Program, pick: &dyn Fn(&Expr) -> bool) -> Span {
        let mut found = None;
        crate::parser::ast::visit::walk_nodes(program, &mut |node| {
            if let crate::parser::ast::visit::Node::Expr(expr) = node {
                if found.is_none() && pick(expr) {
                    found = Some(expr.span);
                }
            }
        });
        found.expect("expression present")
    }

    fn world<'a>(
        class_files: &'a HashMap<String, String>,
        deferrable: &'a HashSet<PathBuf>,
        deferred: &'a HashSet<PathBuf>,
        class_parents: &'a [(String, Vec<String>)],
        lookup: &'a dyn Fn(&str) -> bool,
    ) -> World<'a> {
        World {
            class_files,
            deferrable,
            deferred,
            class_parents,
            autoload_lookup: lookup,
        }
    }

    /// A `return $w->m()` on a class the world holds in full becomes a helper call, keeps its
    /// span, and is reported with the file and line.
    #[test]
    fn undefined_method_on_a_held_class_is_compiled_through_the_helper() {
        let mut ast = program(
            "<?php class W {} class C { function d(W $w) { return $w->walk($this, 1); } }",
        );
        let span = span_of(&ast, &|expr| matches!(expr.kind, ExprKind::MethodCall { .. }));
        let class_files: HashMap<String, String> =
            [("c".to_string(), "/tmp/C.php".to_string())].into_iter().collect();
        let deferrable: HashSet<PathBuf> = [PathBuf::from("/tmp/C.php")].into_iter().collect();
        let deferred = HashSet::default();
        let parents: Vec<(String, Vec<String>)> = Vec::new();
        let lookup = |_: &str| false;
        let error = CompileError::new(span, "Undefined method: W::walk")
            .with_runtime_fault(RuntimeFault::UndefinedMethod {
                class: "W".into(),
                method: "walk".into(),
            })
            .within_declaration("C");
        let mut attempted = HashSet::default();
        let demotions = demote(
            &mut ast,
            &error,
            &world(&class_files, &deferrable, &deferred, &parents, &lookup),
            &mut attempted,
        );
        assert_eq!(demotions.len(), 1);
        assert!(demotions[0].note().contains("'/tmp/C.php' is compiled; 1:"));
        let rewritten = span_of(&ast, &|expr| {
            matches!(&expr.kind, ExprKind::FunctionCall { name, args }
                if name.as_str() == METHOD_CALL_HELPER && args.len() == 4)
        });
        assert_eq!(rewritten, span);
        // The second time the same error is seen, it is not tried again.
        assert!(demote(
            &mut ast,
            &error,
            &world(&class_files, &deferrable, &deferred, &parents, &lookup),
            &mut attempted
        )
        .is_empty());
    }

    /// A receiver class with a subclass in a deferred file is not vouched for: the deferred
    /// subclass may declare the method.
    #[test]
    fn a_class_with_a_deferred_subclass_is_left_to_deferral() {
        let mut ast = program("<?php class W {} class C { function d(W $w) { return $w->walk(); } }");
        let span = span_of(&ast, &|expr| matches!(expr.kind, ExprKind::MethodCall { .. }));
        let class_files: HashMap<String, String> = [
            ("c".to_string(), "/tmp/C.php".to_string()),
            ("sub".to_string(), "/tmp/Sub.php".to_string()),
        ]
        .into_iter().collect();
        let deferrable: HashSet<PathBuf> =
            [PathBuf::from("/tmp/C.php"), PathBuf::from("/tmp/Sub.php")].into_iter().collect();
        let deferred: HashSet<PathBuf> = [PathBuf::from("/tmp/Sub.php")].into_iter().collect();
        let parents = vec![("Sub".to_string(), vec!["W".to_string()])];
        let lookup = |_: &str| false;
        let error = CompileError::new(span, "Undefined method: W::walk")
            .with_runtime_fault(RuntimeFault::UndefinedMethod {
                class: "W".into(),
                method: "walk".into(),
            })
            .within_declaration("C");
        let demotions = demote(
            &mut ast,
            &error,
            &world(&class_files, &deferrable, &deferred, &parents, &lookup),
            &mut HashSet::default(),
        );
        assert!(demotions.is_empty());
    }

    /// A class extending a builtin is not held in full: php may find the member on the builtin
    /// parent, which the checker's model lacks (`DatePoint extends \\DateTimeImmutable`).
    #[test]
    fn a_class_with_a_builtin_ancestor_is_left_to_deferral() {
        let mut ast = program(
            "<?php class Point extends \\DateTimeImmutable { function d() { return $this->__unserialize([]); } }",
        );
        let span = span_of(&ast, &|expr| matches!(expr.kind, ExprKind::MethodCall { .. }));
        let class_files: HashMap<String, String> =
            [("point".to_string(), "/tmp/Point.php".to_string())].into_iter().collect();
        let deferrable: HashSet<PathBuf> = [PathBuf::from("/tmp/Point.php")].into_iter().collect();
        let deferred = HashSet::default();
        let parents = vec![("Point".to_string(), vec!["\\DateTimeImmutable".to_string()])];
        let lookup = |_: &str| false;
        let error = CompileError::new(span, "Undefined method: Point::__unserialize")
            .with_runtime_fault(RuntimeFault::UndefinedMethod {
                class: "Point".into(),
                method: "__unserialize".into(),
            })
            .within_declaration("Point");
        let demotions = demote(
            &mut ast,
            &error,
            &world(&class_files, &deferrable, &deferred, &parents, &lookup),
            &mut HashSet::default(),
        );
        assert!(demotions.is_empty(), "{demotions:?}");
    }

    /// A builtin INTERFACE above the class holds no members, so a method or property fault on
    /// such a class is still vouched for (`PhpParser\NodeAbstract implements \JsonSerializable`
    /// kept every psysh `->parts` read deferred); a constant fault is not, since an interface
    /// can declare constants the checker's model may lack.
    #[test]
    fn a_builtin_interface_ancestor_only_blocks_constant_faults() {
        let source = "<?php class Node implements \\JsonSerializable { function jsonSerialize(): mixed { return 1; } } class C { function d(Node $n) { return $n->parts; } function e() { return Node::K; } }";
        let class_files: HashMap<String, String> =
            [("c".to_string(), "/tmp/C.php".to_string())].into_iter().collect();
        let deferrable: HashSet<PathBuf> = [PathBuf::from("/tmp/C.php")].into_iter().collect();
        let deferred = HashSet::default();
        let parents = vec![("Node".to_string(), vec!["\\JsonSerializable".to_string()])];
        let lookup = |_: &str| false;
        let mut ast = program(source);
        let property_span = span_of(&ast, &|expr| matches!(expr.kind, ExprKind::PropertyAccess { .. }));
        let constant_span = span_of(&ast, &|expr| matches!(expr.kind, ExprKind::ScopedConstantAccess { .. }));
        let mut error = CompileError::new(property_span, "Undefined property: Node::parts")
            .with_runtime_fault(RuntimeFault::UndefinedProperty {
                class: "Node".into(),
                property: "parts".into(),
            });
        error.related = vec![CompileError::new(constant_span, "Undefined class constant: Node::K")
            .with_runtime_fault(RuntimeFault::UndefinedClassConstant {
                class: "Node".into(),
                constant: "K".into(),
            })];
        let error = error.within_declaration("C");
        let demotions = demote(
            &mut ast,
            &error,
            &world(&class_files, &deferrable, &deferred, &parents, &lookup),
            &mut HashSet::default(),
        );
        assert_eq!(demotions.len(), 1, "{demotions:?}");
        assert!(demotions[0].outcome.contains("`->parts`"));
    }

    /// `return X::m();` becomes a throw STATEMENT, and so does `return strlen(X::K)`, since
    /// nothing with an effect runs before the constant; `static::` is left alone; a class the
    /// autoloader could load is too.
    #[test]
    fn static_faults_become_throws_unless_late_bound_or_loadable() {
        let mut ast = program(
            "<?php class X {} class C { function a() { return X::m(); } function b() { return strlen(X::K); } function c() { return static::m(); } function d() { return new Nope(); } }",
        );
        let class_files: HashMap<String, String> =
            [("c".to_string(), "/tmp/C.php".to_string())].into_iter().collect();
        let deferrable: HashSet<PathBuf> = [PathBuf::from("/tmp/C.php")].into_iter().collect();
        let deferred = HashSet::default();
        let parents: Vec<(String, Vec<String>)> = Vec::new();
        let lookup = |name: &str| name == "Nope";
        let method_span = span_of(&ast, &|expr| {
            matches!(&expr.kind, ExprKind::StaticMethodCall { receiver: StaticReceiver::Named(_), .. })
        });
        let constant_span = span_of(&ast, &|expr| matches!(expr.kind, ExprKind::ScopedConstantAccess { .. }));
        let late_span = span_of(&ast, &|expr| {
            matches!(&expr.kind, ExprKind::StaticMethodCall { receiver: StaticReceiver::Static, .. })
        });
        let new_span = span_of(&ast, &|expr| matches!(expr.kind, ExprKind::NewObject { .. }));
        let mut error = CompileError::new(method_span, "Undefined method: X::m")
            .with_runtime_fault(RuntimeFault::UndefinedStaticMethod {
                class: "X".into(),
                method: "m".into(),
            });
        error.related = vec![
            CompileError::new(constant_span, "Undefined class constant: X::K").with_runtime_fault(
                RuntimeFault::UndefinedClassConstant {
                    class: "X".into(),
                    constant: "K".into(),
                },
            ),
            CompileError::new(late_span, "Undefined method: C::m").with_runtime_fault(
                RuntimeFault::UndefinedStaticMethod {
                    class: "C".into(),
                    method: "m".into(),
                },
            ),
            CompileError::new(new_span, "Undefined class: Nope")
                .with_runtime_fault(RuntimeFault::UnknownClass { class: "Nope".into() }),
        ];
        let error = error.within_declaration("C");
        let demotions = demote(
            &mut ast,
            &error,
            &world(&class_files, &deferrable, &deferred, &parents, &lookup),
            &mut HashSet::default(),
        );
        assert_eq!(demotions.len(), 2, "{demotions:?}");
        assert!(demotions[0].outcome.contains("Call to undefined method X::m()"));
        assert!(demotions[1].outcome.contains("Undefined constant X::K"));
        let mut throw_stmts = 0;
        let mut throw_exprs = 0;
        crate::parser::ast::visit::walk_nodes(&ast, &mut |node| match node {
            crate::parser::ast::visit::Node::Stmt(stmt) if matches!(stmt.kind, StmtKind::Throw(_)) => {
                throw_stmts += 1
            }
            crate::parser::ast::visit::Node::Expr(expr) if matches!(expr.kind, ExprKind::Throw(_)) => {
                throw_exprs += 1
            }
            _ => {}
        });
        assert_eq!((throw_stmts, throw_exprs), (2, 0));
    }
}
