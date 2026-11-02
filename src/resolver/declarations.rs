//! Purpose:
//! Extracts and strips declarations that can be discovered from included files.
//! Separates declaration availability from runtime execution of include statements.
//!
//! Called from:
//! - `crate::resolver::discovery` and include resolution paths.
//!
//! Key details:
//! - Discoverable declarations must keep namespace context and include-loaded function variant metadata.

use std::path::Path;

use crate::names::{canonical_name_for_decl, Name};
use crate::parser::ast::{ClassLikeKind, Stmt, StmtKind};

use super::discovery::{FunctionVariantKey, FunctionVariantRegistry};
use super::state::namespace_string;

/// Recursively extracts top-level and namespace-scoped declarations that can be
/// discovered from included files, preserving their namespace and use contexts.
/// Returns declarations wrapped in NamespaceBlock or Synthetic nodes to retain scoping.
pub(super) fn extract_discoverable_declarations(stmts: &[Stmt]) -> Vec<Stmt> {
    let mut declarations = Vec::new();
    let mut context = Vec::new();
    let mut context_flushed = false;

    for stmt in stmts {
        let _source_mode = crate::source::scoped_parse_mode(stmt.profile());
        match &stmt.kind {
            StmtKind::NamespaceDecl { .. } => {
                context.clear();
                context.push(stmt.clone());
                context_flushed = false;
            }
            StmtKind::UseDecl { .. } => {
                context.push(stmt.clone());
                context_flushed = false;
            }
            StmtKind::NamespaceBlock { name, body } => {
                let body_declarations = extract_discoverable_declarations(body);
                if !body_declarations.is_empty() {
                    declarations.push(Stmt::new(
                        StmtKind::NamespaceBlock {
                            name: name.clone(),
                            body: body_declarations,
                        },
                        stmt.span,
                    ));
                }
            }
            StmtKind::Synthetic(body) => {
                let body_declarations = extract_discoverable_declarations(body);
                if !body_declarations.is_empty() {
                    if !context_flushed {
                        declarations.extend(context.clone());
                        context_flushed = true;
                    }
                    declarations.push(Stmt::new(StmtKind::Synthetic(body_declarations), stmt.span));
                }
            }
            kind if is_discoverable_declaration(kind) => {
                if !context_flushed {
                    declarations.extend(context.clone());
                    context_flushed = true;
                }
                declarations.push(stmt.clone());
            }
            _ => {}
        }
    }

    declarations
}

/// Removes discoverable declarations from the statement list, replacing function
/// declarations with FunctionVariantMark nodes that record the include-loaded variant.
/// Uses `canonical` path and `function_variants` registry to determine which variant
/// should be active in the including file's scope. File-level functions bind before
/// the file body, including declarations in later namespace blocks. Existing child
/// include markers and conditional declarations retain their execution positions.
pub(super) fn strip_discoverable_declarations(
    stmts: Vec<Stmt>,
    canonical: Option<&Path>,
    function_variants: &FunctionVariantRegistry,
    outcome: &mut ConditionalDeclarationOutcome,
) -> Vec<Stmt> {
    let mut early_bindings = Vec::new();
    let body = strip_stmts(
        stmts,
        canonical,
        function_variants,
        None,
        &mut early_bindings,
        true,
        outcome,
    );
    early_bindings.extend(body);
    early_bindings
}

/// Internal recursive helper that processes statements and strips discoverable
/// declarations, tracking the current namespace context via `current_namespace`.
/// The `namespace` parameter carries the effective namespace for the current block.
fn strip_stmts(
    stmts: Vec<Stmt>,
    canonical: Option<&Path>,
    function_variants: &FunctionVariantRegistry,
    namespace: Option<String>,
    early_bindings: &mut Vec<Stmt>,
    allow_early_bindings: bool,
    outcome: &mut ConditionalDeclarationOutcome,
) -> Vec<Stmt> {
    let mut stripped = Vec::new();
    let mut namespace = namespace;
    for stmt in stmts {
        let _source_mode = crate::source::scoped_parse_mode(stmt.profile());
        let stmt_namespace = namespace.clone();
        // Only this file's declarations bind before its body. Existing marks
        // belong to already-expanded child includes and keep their execution site.
        let is_function = matches!(&stmt.kind, StmtKind::FunctionDecl { .. });
        let is_early_interface = matches!(
            &stmt.kind,
            StmtKind::InterfaceDecl { extends, .. } if extends.is_empty()
        );
        if let Some(stmt) = strip_stmt(
            stmt,
            canonical,
            function_variants,
            stmt_namespace.as_deref(),
            &mut namespace,
            early_bindings,
            allow_early_bindings,
            outcome,
        ) {
            if allow_early_bindings && (is_function || is_early_interface) {
                push_early_binding(early_bindings, stmt, stmt_namespace.as_deref());
            } else {
                stripped.push(stmt);
            }
        }
    }
    stripped
}

/// What a strip did with the conditional declarations it walked past.
///
/// Both halves are needed, and one without the other misleads. A polyfill's version guard has two
/// branches declaring the SAME function; only the live one is bound, and reading the dead branch's
/// drop alone says the function was lost when it was not.
#[derive(Default)]
pub(super) struct ConditionalDeclarationOutcome {
    /// Names whose conditional declaration was dropped instead of bound.
    pub(super) dropped: Vec<String>,
    /// Names bound through the function-variant mechanism.
    pub(super) bound: Vec<String>,
}

/// Returns whether an INCLUDED file's conditionally declared functions are kept for binding.
///
/// ON. Keeping those bodies is what makes `if (!function_exists('f')) { function f() {…} }` in an
/// included file behave as php does — the shape every polyfill uses, and the one Symfony reaches
/// through `polyfill-mbstring/bootstrap.php` -> `bootstrap80.php`. It widens the closed world to
/// every class those bodies name, which is correct: php reaches the same code.
///
/// It was off while one such body (`symfony/polyfill-deepclone`) reached PHP elephc could not
/// compile or ran wrongly: references stored in array elements (`[&$refs[$k], $value, &$value]`,
/// `$value[$k] = &$refs[$rid]`), forward `goto` into a loop body's tail, runtime-named first-class
/// callables and property references, and reflection through boxed reflectors. Those now compile
/// and run as php does (`tests/codegen/deepclone_shapes.rs`, `tests/codegen/array_references.rs`).
///
/// `ELEPHC_BIND_CONDITIONAL_INCLUDE_DECLARATIONS=0` (or `off` / `false`) turns it back off, to
/// bisect a regression the widened world exposes.
fn conditional_include_declarations_are_bound() -> bool {
    !matches!(
        std::env::var("ELEPHC_BIND_CONDITIONAL_INCLUDE_DECLARATIONS").as_deref(),
        Ok("0") | Ok("off") | Ok("false")
    )
}

/// Processes a single statement, removing discoverable declarations while
/// preserving namespace declarations and blocks. Function declarations are
/// replaced with FunctionVariantMark using the canonical path and registry to
/// resolve the correct variant. Updates `current_namespace` when entering a
/// NamespaceDecl.
fn strip_stmt(
    stmt: Stmt,
    canonical: Option<&Path>,
    function_variants: &FunctionVariantRegistry,
    namespace: Option<&str>,
    current_namespace: &mut Option<String>,
    early_bindings: &mut Vec<Stmt>,
    allow_early_bindings: bool,
    outcome: &mut ConditionalDeclarationOutcome,
) -> Option<Stmt> {
    let span = stmt.span;
    match stmt.kind {
        StmtKind::FunctionDecl { .. } => {
            let name = match &stmt.kind {
                StmtKind::FunctionDecl { name, .. } => name.as_str(),
                _ => unreachable!("matched FunctionDecl"),
            };
            let public_name = canonical_name_for_decl(namespace, name);
            let variant = canonical.and_then(|canonical| {
                function_variants.get(&FunctionVariantKey::new(canonical, &public_name))
            });
            match variant {
                Some(variant) => {
                    // Recorded beside the drops: a name bound HERE answers for the same name
                    // dropped in a sibling file, which is what a version guard's two branches do.
                    outcome.bound.push(public_name.clone());
                    Some(Stmt::new(
                    StmtKind::FunctionVariantMark {
                        name: variant.public_name.clone(),
                        variant: variant.variant_name.clone(),
                    },
                    span,
                ))
                }
                // A declaration in an unconditional position was already hoisted by
                // `extract_discoverable_declarations`, so removing it here is what keeps the
                // file from declaring it twice. One nested in a conditional statement was NOT
                // hoisted — php binds it only when that statement executes — so removing it
                // would lose the body outright and the including file would be told the
                // function does not exist. It stays where it is; `crate::conditional_functions`
                // gives it its execution-time binding once the program is whole.
                None if !allow_early_bindings && conditional_include_declarations_are_bound() => {
                    Some(stmt)
                }
                None => {
                    // Dropping it is what the gate above costs, and the cost can be invisible:
                    // the file still reports as one the compiler included, the runtime skips it
                    // on that word, and the function ends up declared by nobody.
                    if !allow_early_bindings {
                        outcome.dropped.push(public_name.clone());
                    }
                    None
                }
            }
        }
        StmtKind::InterfaceDecl { name, .. } => canonical.map(|canonical| {
            Stmt::new(
                StmtKind::ClassLikeActivate {
                    name,
                    kind: ClassLikeKind::Interface,
                    source_path: canonical.to_path_buf(),
                },
                span,
            )
        }),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let nested_namespace = namespace.map(str::to_string);
            let then_body = strip_stmts(
                then_body,
                canonical,
                function_variants,
                nested_namespace.clone(),
                early_bindings,
                false,
                &mut *outcome,
            );
            let elseif_clauses = elseif_clauses
                .into_iter()
                .map(|(condition, body)| {
                    (
                        condition,
                        strip_stmts(
                            body,
                            canonical,
                            function_variants,
                            nested_namespace.clone(),
                            early_bindings,
                            false,
                            &mut *outcome,
                        ),
                    )
                })
                .collect();
            let else_body = else_body.map(|body| {
                strip_stmts(
                    body,
                    canonical,
                    function_variants,
                    nested_namespace,
                    early_bindings,
                    false,
                    &mut *outcome,
                )
            });
            Some(Stmt::new(
                StmtKind::If {
                    condition,
                    then_body,
                    elseif_clauses,
                    else_body,
                },
                span,
            ))
        }
        StmtKind::While { condition, body } => Some(Stmt::new(
            StmtKind::While {
                condition,
                body: strip_deferred_body(
                    body,
                    canonical,
                    function_variants,
                    namespace,
                    early_bindings,
                                &mut *outcome,
                            ),
            },
            span,
        )),
        StmtKind::DoWhile { body, condition } => Some(Stmt::new(
            StmtKind::DoWhile {
                body: strip_deferred_body(
                    body,
                    canonical,
                    function_variants,
                    namespace,
                    early_bindings,
                                &mut *outcome,
                            ),
                condition,
            },
            span,
        )),
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => Some(Stmt::new(
            StmtKind::For {
                init,
                condition,
                update,
                body: strip_deferred_body(
                    body,
                    canonical,
                    function_variants,
                    namespace,
                    early_bindings,
                                &mut *outcome,
                            ),
            },
            span,
        )),
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            value_by_ref,
            body,
        } => Some(Stmt::new(
            StmtKind::Foreach {
                array,
                key_var,
                value_var,
                value_by_ref,
                body: strip_deferred_body(
                    body,
                    canonical,
                    function_variants,
                    namespace,
                    early_bindings,
                                &mut *outcome,
                            ),
            },
            span,
        )),
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => Some(Stmt::new(
            StmtKind::Switch {
                subject,
                cases: cases
                    .into_iter()
                    .map(|(patterns, body)| {
                        (
                            patterns,
                            strip_deferred_body(
                                body,
                                canonical,
                                function_variants,
                                namespace,
                                early_bindings,
                                                        &mut *outcome,
                                                    ),
                        )
                    })
                    .collect(),
                default: default.map(|body| {
                    strip_deferred_body(
                        body,
                        canonical,
                        function_variants,
                        namespace,
                        early_bindings,
                                        &mut *outcome,
                                    )
                }),
            },
            span,
        )),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => Some(Stmt::new(
            StmtKind::Try {
                try_body: strip_deferred_body(
                    try_body,
                    canonical,
                    function_variants,
                    namespace,
                    early_bindings,
                                &mut *outcome,
                            ),
                catches: catches
                    .into_iter()
                    .map(|mut catch| {
                        catch.body = strip_deferred_body(
                            catch.body,
                            canonical,
                            function_variants,
                            namespace,
                            early_bindings,
                                                &mut *outcome,
                                            );
                        catch
                    })
                    .collect(),
                finally_body: finally_body.map(|body| {
                    strip_deferred_body(
                        body,
                        canonical,
                        function_variants,
                        namespace,
                        early_bindings,
                                        &mut *outcome,
                                    )
                }),
            },
            span,
        )),
        // A class-like nested in a conditional statement was not hoisted by
        // `extract_discoverable_declarations`, which only takes unconditional positions; removing
        // it lost the class outright (Carbon's `if (!class_exists(DatePeriodBase::class, false))
        // { class DatePeriodBase … }`). It stays where php declares it, and
        // `crate::conditional_classes` decides its binding once the program is whole.
        kind if is_conditional_class_like(&kind)
            && !allow_early_bindings
            && conditional_include_declarations_are_bound() =>
        {
            Some(Stmt { kind, ..stmt })
        }
        kind if is_discoverable_declaration(&kind) => None,
        StmtKind::NamespaceDecl { name } => {
            *current_namespace = Some(namespace_string(&name));
            Some(Stmt::new(StmtKind::NamespaceDecl { name }, span))
        }
        StmtKind::NamespaceBlock { name, body } => Some(Stmt::new(
            StmtKind::NamespaceBlock {
                body: strip_stmts(
                    body,
                    canonical,
                    function_variants,
                    Some(namespace_string(&name)),
                    early_bindings,
                    true,
                    &mut *outcome,
                ),
                name,
            },
            span,
        )),
        StmtKind::Synthetic(body) => {
            let body = strip_stmts(
                body,
                canonical,
                function_variants,
                current_namespace.clone(),
                early_bindings,
                allow_early_bindings,
                &mut *outcome,
            );
            if body.is_empty() {
                None
            } else {
                Some(Stmt::new(StmtKind::Synthetic(body), span))
            }
        }
        other => Some(Stmt::new(other, span)),
    }
}

/// Rewrites declaration events in an executable nested body without promoting
/// them into the file's early-binding prologue.
fn strip_deferred_body(
    body: Vec<Stmt>,
    canonical: Option<&Path>,
    function_variants: &FunctionVariantRegistry,
    namespace: Option<&str>,
    early_bindings: &mut Vec<Stmt>,
    outcome: &mut ConditionalDeclarationOutcome,
) -> Vec<Stmt> {
    strip_stmts(
        body,
        canonical,
        function_variants,
        namespace.map(str::to_string),
        early_bindings,
        false,
        outcome,
    )
}

/// Keeps an early declaration's namespace while moving it before the file body.
///
/// Name resolution runs after inclusion rewriting. A raw local declaration name
/// cannot be moved ahead of its namespace statement without changing its PHP
/// symbol, so a transparent namespace block carries the original resolution
/// context instead of reimplementing the name resolver here.
fn push_early_binding(
    early_bindings: &mut Vec<Stmt>,
    stmt: Stmt,
    namespace: Option<&str>,
) {
    let Some(namespace) = namespace.filter(|namespace| !namespace.is_empty()) else {
        early_bindings.push(stmt);
        return;
    };
    let name = Name::qualified(namespace.split('\\').map(str::to_string).collect());
    let span = stmt.span;
    early_bindings.push(Stmt::new(
        StmtKind::NamespaceBlock {
            name: Some(name),
            body: vec![stmt],
        },
        span,
    ));
}

/// Returns true for the class-like declarations `crate::conditional_classes` can bind from a
/// conditional position.
fn is_conditional_class_like(kind: &StmtKind) -> bool {
    matches!(
        kind,
        StmtKind::ClassDecl { .. }
            | StmtKind::EnumDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. }
    )
}

/// Returns true if the statement kind is a discoverable declaration that
/// should be extracted/stripped during include processing.
fn is_discoverable_declaration(kind: &StmtKind) -> bool {
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
