//! Purpose:
//! Models catchable throws raised by PHP destructor execution for exception-flow DCE.
//! Attributes `__destruct` throws to the sites that retire a value: call operands, discarded
//! statement results, retired receivers, retired callable literals, rebinding writes, and
//! callable scope teardown.
//!
//! Called from:
//! - `crate::optimize::exception_flow::ExceptionFlowAnalysis::from_program()`
//! - `crate::optimize::exception_flow::ExceptionFlowAnalysis::expr_throws()`
//! - `crate::optimize::exception_flow::ExceptionFlowAnalysis::stmt_throws()`
//! - `crate::optimize::exception_flow::ExceptionFlowAnalysis::summarize_bodies()`
//!
//! Key details:
//! - A closed program without a throwing destructor keeps an empty destruction summary.
//!   Opaque eval or an unsummarized destructor opens that summary conservatively.
//! - A `new C()` temporary names an EXACT runtime class, so its own destructor resolves through
//!   the parent chain precisely. `new static()` does NOT: late static binding can name a
//!   subclass whose `__destruct` override throws something else, which is why
//!   `resolve_exception_receiver` answers `None` for it and this module then falls back to the
//!   program-wide summary. `new self()`/`new parent()` are early-bound and stay exact.
//! - Destroying an object also retires the instance storage it owns. Only a proven non-heap
//!   scalar layout (`ExceptionHierarchy::class_owns_no_destructible_storage`) removes that term;
//!   "the class declares no `__destruct` of its own" is NOT a proof that its children are quiet.
//! - Destructor enumeration is proven against the authoritative `opcache_prelude::detect`
//!   traversal, which reaches trait bodies, conditional declarations, and classes declared
//!   inside closure bodies. Anything it finds that the summary collector did not summarize
//!   opens the gate to the unknown throwable domain, and so does any reachable `eval`.
//! - Scope cleanup is omitted only for a callable whose frame is PROVEN to own nothing
//!   destructible. That proof is deliberately narrow (see
//!   [`frame_retires_nothing_destructible`]); everything it cannot judge keeps the conservative
//!   program-wide summary.

use super::*;
use crate::names::php_symbol_key;
use crate::opcache_prelude::detect;
use crate::optimize::effect_analysis::method_effect_key;
use crate::parser::ast::{
    CallableTarget, Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind, TypeExpr,
};
use crate::types::PhpType;
use std::collections::{HashMap, HashSet};

use super::hierarchy::type_expr_is_non_destructible;

/// Method-key suffix identifying a destructor summary in the instance-method map.
const DESTRUCTOR_KEY_SUFFIX: &str = "::__destruct";

/// PHP's destructor magic-method name, compared through the shared case-insensitive key.
const DESTRUCTOR_METHOD: &str = "__destruct";

/// PHP's opaque dynamic-source construct, spelled as an ordinary call in the AST.
const EVAL_FUNCTION: &str = "eval";

/// Call forms that can bind NEW frame storage under a name the body never spells out.
///
/// `extract()` writes locals chosen at runtime and `eval()` can declare and leave behind
/// anything at all, so a body containing either is never proven to own only its parameters.
/// `extract()` is additionally rejected by the by-reference argument rule below, but naming it
/// here keeps the reason visible where it belongs.
const FRAME_WRITING_CALLS: [&str; 2] = ["extract", EVAL_FUNCTION];

impl ExceptionFlowAnalysis {
    /// Returns throws that retiring ANY value in this program may raise.
    ///
    /// This is the conservative fallback every imprecise destruction position uses. It is empty
    /// for a program whose destructors cannot throw, a precise union of the summarized
    /// destructor bodies when enumeration succeeded, and the unknown throwable domain when a
    /// destructor exists that the fixed point could not summarize.
    pub(super) fn any_destructor_throws(&self) -> ThrownTypes {
        self.destructor_throws.clone()
    }

    /// Returns throws raised while the named callable's frame retires its parameters and locals.
    ///
    /// PHP drops a frame's remaining owners when the callable returns, so any value the frame
    /// still owns runs its destructor inside the CALLEE and propagates to the caller. A callable
    /// proven to own nothing destructible (see [`frame_retires_nothing_destructible`]) adds
    /// nothing here; everything else falls back to the program-wide summary, which is still
    /// empty for a program whose destructors cannot throw.
    ///
    /// `$this` is deliberately NOT part of this term. The receiver is owned by the CALLER: a
    /// `(new C())->m()` temporary is retired at the call site, which is where
    /// [`Self::temporary_destruction_throws`] already accounts for it. Charging the callee frame
    /// for it too would double-count the same retirement and would make every instance method
    /// unprovable for no soundness gain.
    pub(super) fn scope_cleanup_throws(&self, callable_key: &str) -> ThrownTypes {
        if self.scalar_frame_callables.contains(callable_key) {
            ThrownTypes::default()
        } else {
            self.any_destructor_throws()
        }
    }

    /// Returns throws raised when a write retires the value its target previously held.
    ///
    /// Rebinding a variable, a static property, or a list target drops the old value, and
    /// `unset` does the same explicitly. Whether the retired value was an object is not
    /// derivable from data this pass has, so the term is the program-wide summary rather than an
    /// invented per-variable claim.
    pub(super) fn overwrite_cleanup_throws(&self) -> ThrownTypes {
        self.any_destructor_throws()
    }

    /// Recomputes the program-wide destructor summary from the current fixed-point round.
    ///
    /// Unions every summarized `__destruct` body and falls back to the unknown throwable domain
    /// when the hierarchy reports a destructor source the fixed point cannot see.
    pub(super) fn compute_destructor_throws(&self) -> ThrownTypes {
        let mut thrown = ThrownTypes::default();
        for (method_key, summary) in &self.instance_method_throws {
            if method_key.ends_with(DESTRUCTOR_KEY_SUFFIX) {
                thrown = thrown.combined(summary.clone());
            }
        }
        if self.hierarchy.destructor_sources_are_open() {
            thrown = thrown.combined(ThrownTypes::unknown());
        }
        thrown
    }

    /// Computes throws raised while retiring one instance of an exactly known runtime class.
    ///
    /// Only `new C()`, `new self()`, and `new parent()` reach this path, so `class_name` is the
    /// exact runtime class and the parent-chain lookup of `__destruct` is precise. The instance
    /// storage the object owns is a SEPARATE term: a class without its own destructor still
    /// retires its properties, and only a proven scalar-only layout removes that.
    fn exact_class_destruction_throws(&self, class_name: &str) -> ThrownTypes {
        let own_destructor = match self.resolve_method_value(
            class_name,
            DESTRUCTOR_METHOD,
            &self.instance_method_throws,
        ) {
            Some(summary) => summary,
            None if self.hierarchy.method_lookup_is_closed(class_name)
                && self.hierarchy.is_declared_class(class_name) =>
            {
                ThrownTypes::default()
            }
            None => return self.any_destructor_throws(),
        };
        if self.hierarchy.class_owns_no_destructible_storage(class_name) {
            own_destructor
        } else {
            own_destructor.combined(self.any_destructor_throws())
        }
    }

    /// Computes throws raised while retiring a value of one declared PHP type.
    ///
    /// A declared object type can name a superclass of the runtime class, so object, `mixed`,
    /// `iterable`, and `callable` positions stay conservative. Array types recurse into their
    /// children because retiring a container retires the children it owns. The scalar arms are
    /// the precision that keeps a scalar-returning callee from resurrecting a catch.
    fn type_destruction_throws(&self, php_type: &PhpType) -> ThrownTypes {
        match php_type {
            PhpType::Object(_)
            | PhpType::Mixed
            | PhpType::Iterable
            | PhpType::Callable
            | PhpType::Resource(_) => self.any_destructor_throws(),
            PhpType::Array(element) => self.type_destruction_throws(element),
            PhpType::AssocArray { key, value } => self
                .type_destruction_throws(key)
                .combined(self.type_destruction_throws(value)),
            PhpType::Union(members) => members.iter().fold(ThrownTypes::default(), |thrown, member| {
                thrown.combined(self.type_destruction_throws(member))
            }),
            PhpType::Int
            | PhpType::Float
            | PhpType::Str
            | PhpType::Bool
            | PhpType::False
            | PhpType::Void
            | PhpType::Never
            | PhpType::Buffer(_)
            | PhpType::Packed(_)
            | PhpType::Pointer(_)
            | PhpType::TaggedScalar => ThrownTypes::default(),
        }
    }

    /// Computes throws raised while retiring the value an expression leaves at this site.
    ///
    /// Literals and non-heap operator forms own nothing, which is what keeps scalar-only code
    /// unaffected. A variable is NOT in that set: a borrowed `mixed`/array/object view becomes
    /// the last owned reference as soon as another argument or the callee rebinds the source,
    /// and this pass has no ownership proof that says otherwise. Unrecognized forms fall back to
    /// the program-wide summary.
    pub(super) fn temporary_destruction_throws(
        &self,
        expr: &Expr,
        class_context: Option<&ExceptionClassContext>,
    ) -> ThrownTypes {
        match &expr.kind {
            // Non-heap operands: there is no runtime value here for this site to retire.
            ExprKind::StringLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
            | ExprKind::MagicConstant(_)
            | ExprKind::InstanceOf { .. }
            | ExprKind::Not(_)
            | ExprKind::BitNot(_)
            | ExprKind::Negate(_)
            | ExprKind::PreIncrement(_)
            | ExprKind::PostIncrement(_)
            | ExprKind::PreDecrement(_)
            | ExprKind::PostDecrement(_)
            | ExprKind::ObjectClassName { .. } => ThrownTypes::default(),
            // A callable literal IS a runtime value, and this site retires it. Its captures and
            // its bound receiver go with it.
            ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_) => {
                self.callable_value_retirement_throws(expr, class_context)
            }
            ExprKind::NewObject { class_name, .. } => {
                self.exact_class_destruction_throws(class_name.as_str())
            }
            ExprKind::NewScopedObject { receiver, .. } => {
                resolve_exception_receiver(receiver, class_context)
                    .map(|class_name| self.exact_class_destruction_throws(&class_name))
                    .unwrap_or_else(|| self.any_destructor_throws())
            }
            // A fresh container is retired with the children it owns, so the elements are the
            // exact destruction sites rather than the literal itself.
            ExprKind::ArrayLiteral(items) => items.iter().fold(ThrownTypes::default(), |thrown, item| {
                thrown.combined(self.temporary_destruction_throws(item, class_context))
            }),
            ExprKind::ArrayLiteralAssoc(items) => {
                items.iter().fold(ThrownTypes::default(), |thrown, (key, value)| {
                    thrown
                        .combined(self.temporary_destruction_throws(key, class_context))
                        .combined(self.temporary_destruction_throws(value, class_context))
                })
            }
            ExprKind::ErrorSuppress(inner)
            | ExprKind::Spread(inner)
            | ExprKind::NamedArg { value: inner, .. }
            | ExprKind::Cast { expr: inner, .. } => {
                self.temporary_destruction_throws(inner, class_context)
            }
            // `+` on arrays yields a container whose children come from the operands; every other
            // operator yields a scalar, for which both recursions are empty anyway.
            ExprKind::BinaryOp { left, right, .. } => self
                .temporary_destruction_throws(left, class_context)
                .combined(self.temporary_destruction_throws(right, class_context)),
            ExprKind::FunctionCall { name, .. } => self
                .function_returns
                .get(name.as_str())
                .map(|return_type| self.type_destruction_throws(return_type))
                .unwrap_or_else(|| self.any_destructor_throws()),
            ExprKind::StaticMethodCall {
                receiver, method, ..
            } => resolve_exception_receiver(receiver, class_context)
                .and_then(|class_name| {
                    self.resolve_method_value(&class_name, method, &self.static_method_returns)
                })
                .map(|return_type| self.type_destruction_throws(&return_type))
                .unwrap_or_else(|| self.any_destructor_throws()),
            ExprKind::MethodCall { object, method, .. }
            | ExprKind::NullsafeMethodCall { object, method, .. } => {
                exact_receiver_class(object, class_context)
                    .and_then(|class_name| {
                        self.resolve_method_value(&class_name, method, &self.instance_method_returns)
                    })
                    .map(|return_type| self.type_destruction_throws(&return_type))
                    .unwrap_or_else(|| self.any_destructor_throws())
            }
            ExprKind::Ternary {
                then_expr,
                else_expr,
                ..
            } => self
                .temporary_destruction_throws(then_expr, class_context)
                .combined(self.temporary_destruction_throws(else_expr, class_context)),
            ExprKind::ShortTernary { value, default }
            | ExprKind::NullCoalesce { value, default } => self
                .temporary_destruction_throws(value, class_context)
                .combined(self.temporary_destruction_throws(default, class_context)),
            ExprKind::Match { arms, default, .. } => {
                let mut thrown = arms.iter().fold(ThrownTypes::default(), |thrown, (_, value)| {
                    thrown.combined(self.temporary_destruction_throws(value, class_context))
                });
                if let Some(default) = default {
                    thrown = thrown.combined(self.temporary_destruction_throws(default, class_context));
                }
                thrown
            }
            _ => self.any_destructor_throws(),
        }
    }

    /// Computes throws raised while retiring every value a call's operand list leaves behind.
    ///
    /// PHP retires argument temporaries in the CALLER's frame after the call returns, which is
    /// why a throwing argument destructor is catchable by a `try` in that same frame.
    pub(super) fn operand_cleanup_throws(
        &self,
        args: &[Expr],
        class_context: Option<&ExceptionClassContext>,
    ) -> ThrownTypes {
        args.iter().fold(ThrownTypes::default(), |thrown, arg| {
            thrown.combined(self.temporary_destruction_throws(arg, class_context))
        })
    }

    /// Computes throws raised while retiring a callable LITERAL written at this site.
    ///
    /// A closure literal and a first-class-callable literal both materialize a descriptor that
    /// this site creates and retires, so the descriptor may hold the last owner of everything it
    /// binds:
    ///
    /// - explicit by-value captures (`use ($held)`) and by-reference captures (`use (&$held)`,
    ///   whose shared cell the descriptor also releases);
    /// - an arrow function's IMPLICIT by-value captures, which are not listed in the AST at all,
    ///   so any arrow function is conservative;
    /// - the implicit `$this` a non-static closure written inside a class body binds;
    /// - the receiver a first-class callable binds, which is precise when that receiver is
    ///   itself an exactly known temporary (`(new C())->m(...)`).
    ///
    /// This helper accounts for literal descriptors. Nonliteral callees keep the conservative
    /// result of `callable_expr_throws`, which also covers their possible retirement.
    pub(super) fn callable_value_retirement_throws(
        &self,
        expr: &Expr,
        class_context: Option<&ExceptionClassContext>,
    ) -> ThrownTypes {
        match &expr.kind {
            ExprKind::Closure {
                captures,
                capture_refs,
                is_arrow,
                is_static,
                ..
            } => {
                let binds_values = !captures.is_empty() || !capture_refs.is_empty() || *is_arrow;
                let binds_receiver = !*is_static && class_context.is_some();
                if binds_values || binds_receiver {
                    self.any_destructor_throws()
                } else {
                    ThrownTypes::default()
                }
            }
            ExprKind::FirstClassCallable(target) => match target {
                CallableTarget::Method { object, .. } => {
                    self.temporary_destruction_throws(object, class_context)
                }
                CallableTarget::Function(_) | CallableTarget::StaticMethod { .. } => {
                    ThrownTypes::default()
                }
            },
            _ => ThrownTypes::default(),
        }
    }
}

/// Returns whether the program holds a destructor source the exception summary cannot see.
///
/// Two things open the summary. The first is a `__destruct` the fixed point never summarized;
/// see [`declared_destructor_is_unsummarized`]. The second is a reachable `eval`: its source is
/// opaque to AOT compilation, so it can declare a class with a throwing `__destruct` and hand
/// back an instance that no AST in this program mentions, and the summarized destructor bodies
/// would then be an under-approximation rather than a union.
///
/// The `eval` question rides on the same authoritative `detect` traversal, matched as an
/// ordinary function name so a callable string (`'eval'`) counts too.
///
/// # What this cannot see
///
/// This is a SOURCE test. A capability forced on from the command line (`--with-eval`) with no
/// `eval` call or `'eval'` string anywhere in the program leaves nothing for the traversal to
/// find, so the summary stays closed. That is sound for the program as written (with no source
/// manifestation there is no AOT site at which opaque code can enter), but it does mean this
/// gate describes the SOURCE, not the linked capability set. A future dynamic entry point that
/// is reachable without naming itself in source has to extend this function rather than rely on
/// it.
pub(super) fn destructor_sources_are_open(
    stmts: &[Stmt],
    summarized: &HashMap<String, ThrownTypes>,
) -> bool {
    if detect::first_reference(stmts, detect::Symbol::function(EVAL_FUNCTION)).is_some() {
        return true;
    }
    declared_destructor_is_unsummarized(stmts, summarized)
}

/// Returns whether the program holds a `__destruct` the exception summary collector never saw.
///
/// `summarized` is the fixed point's instance-method key set, which
/// `crate::optimize::exception_flow::callables::collect_exception_bodies` fills from top-level
/// and namespace-block class declarations with method bodies. This function walks the same
/// shapes, judges each `__destruct` it meets against that set, and hands every other statement
/// to the authoritative `detect` traversal, the one exhaustive walk that reaches trait bodies,
/// conditional declarations, and classes declared inside closure bodies, and that fails to
/// compile rather than silently ignoring a new `ExprKind`/`StmtKind`.
///
/// Answering `true` only costs precision: the program-wide destructor summary then widens to the
/// unknown throwable domain and catches stay. Answering `false` when a destructor was missed
/// would drop a reachable catch, so every position this function cannot judge answers `true`.
fn declared_destructor_is_unsummarized(
    stmts: &[Stmt],
    summarized: &HashMap<String, ThrownTypes>,
) -> bool {
    let destructor = detect::Symbol::syntactic(detect::SymbolKind::DestructorDeclaration);
    stmts.iter().any(|stmt| match &stmt.kind {
        // The collector recurses through namespace blocks, so their declarations are judged the
        // same way rather than handed to the conservative fallback.
        StmtKind::NamespaceBlock { body, .. } => {
            declared_destructor_is_unsummarized(body, summarized)
        }
        StmtKind::ClassDecl { name, methods, .. } => methods.iter().any(|method| {
            let declares_destructor = php_symbol_key(&method.name) == DESTRUCTOR_METHOD;
            if declares_destructor
                && (!method.has_body
                    || !summarized.contains_key(&method_effect_key(name, &method.name)))
            {
                return true;
            }
            // A class declared inside a method body never reaches the collector.
            detect::first_reference(&method.body, destructor).is_some()
        }),
        _ => detect::first_reference(std::slice::from_ref(stmt), destructor).is_some(),
    })
}

/// Collects the callables whose frame teardown provably retires nothing destructible.
///
/// Keys match `crate::optimize::exception_flow::callables::collect_exception_bodies` exactly:
/// the raw name for a function, `method_effect_key` for a method. That matters because
/// `ExceptionFlowAnalysis::summarize_bodies` looks the proof up by the same key it summarizes
/// under. A callable this walk never reaches is simply absent, which is the conservative answer.
pub(super) fn collect_scalar_frame_callables(stmts: &[Stmt], proven: &mut HashSet<String>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FunctionDecl {
                name,
                params,
                variadic,
                body,
                ..
            } => {
                if frame_retires_nothing_destructible(params, variadic.as_ref(), body) {
                    proven.insert(name.clone());
                }
            }
            StmtKind::ClassDecl { name, methods, .. } => {
                for method in methods.iter().filter(|method| method.has_body) {
                    if frame_retires_nothing_destructible(
                        &method.params,
                        method.variadic.as_ref(),
                        &method.body,
                    ) {
                        proven.insert(method_effect_key(name, &method.name));
                    }
                }
            }
            StmtKind::NamespaceBlock { body, .. } => collect_scalar_frame_callables(body, proven),
            _ => {}
        }
    }
}

/// Returns whether a callable frame can be proven to own no value with a destructor.
///
/// The proof has two halves, and BOTH must hold.
///
/// *Signature*: every parameter is declared with a non-heap scalar type and passed by value, and
/// there is no variadic parameter. An untyped, by-reference, or object-typed parameter leaves
/// the frame holding storage this pass cannot describe.
///
/// *Body*: no statement binds unproven frame storage; see [`stmt_binds_no_frame_storage`].
/// Named-local increments are a scalar-only exception. Other assignments stay conservative:
/// tracking which local holds what would
/// need a dataflow this AST pass does not have, and a single mis-ordered read would make the
/// answer unsound rather than imprecise. The remaining frame storage is scalar parameters
/// plus any scalar-only increment result.
///
/// Hidden temporaries are not part of this term either: PHP retires an argument or discarded
/// expression temporary AT ITS OWN STATEMENT, which
/// [`ExceptionFlowAnalysis::temporary_destruction_throws`] and
/// [`ExceptionFlowAnalysis::operand_cleanup_throws`] already attribute to that statement.
///
/// The cost of this narrowness is precision only: an unproven callable keeps unioning the
/// program-wide destructor summary into its own summary, exactly as before.
fn frame_retires_nothing_destructible(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    variadic: Option<&String>,
    body: &[Stmt],
) -> bool {
    if variadic.is_some() {
        return false;
    }
    let signature_is_scalar = params.iter().all(|(_, hint, _, by_ref)| {
        !by_ref && hint.as_ref().is_some_and(type_expr_is_non_destructible)
    });
    signature_is_scalar && body.iter().all(stmt_binds_no_frame_storage)
}

/// Returns whether one statement provably binds nothing into the enclosing frame.
///
/// The allowed set is a whitelist and the fallthrough answer is `false`, so a statement form
/// this function has never been taught about refuses the proof instead of silently widening it.
/// That is the opposite polarity from the destructor-enumeration walk above, where a missed form
/// would be unsound and the traversal is therefore exhaustive by construction.
///
/// Everything that creates a name in the frame is refused: assignments, `list()` unpacking,
/// `foreach` loop variables, `global`, `static`, a `catch` variable, and every declaration form.
fn stmt_binds_no_frame_storage(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Break(_) | StmtKind::Continue(_) | StmtKind::Return(None) => true,
        StmtKind::Echo(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::Throw(expr)
        | StmtKind::Return(Some(expr)) => expr_binds_no_frame_storage(expr),
        StmtKind::Synthetic(body) => body.iter().all(stmt_binds_no_frame_storage),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_binds_no_frame_storage(condition)
                && then_body.iter().all(stmt_binds_no_frame_storage)
                && elseif_clauses.iter().all(|(condition, body)| {
                    expr_binds_no_frame_storage(condition)
                        && body.iter().all(stmt_binds_no_frame_storage)
                })
                && else_body
                    .as_deref()
                    .is_none_or(|body| body.iter().all(stmt_binds_no_frame_storage))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expr_binds_no_frame_storage(condition) && body.iter().all(stmt_binds_no_frame_storage)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_none_or(stmt_binds_no_frame_storage)
                && condition.as_ref().is_none_or(expr_binds_no_frame_storage)
                && update.as_deref().is_none_or(stmt_binds_no_frame_storage)
                && body.iter().all(stmt_binds_no_frame_storage)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_binds_no_frame_storage(subject)
                && cases.iter().all(|(patterns, body)| {
                    patterns.iter().all(expr_binds_no_frame_storage)
                        && body.iter().all(stmt_binds_no_frame_storage)
                })
                && default
                    .as_deref()
                    .is_none_or(|body| body.iter().all(stmt_binds_no_frame_storage))
        }
        _ => false,
    }
}

/// Returns whether one expression provably binds nothing into the enclosing frame.
///
/// Reading frame storage is fine; WRITING it is not. Assignments, increments through a
/// reference, generators, closures, and `include` in expression position are all refused by the
/// `false` fallthrough, as is any form not listed here.
///
/// Calls are allowed only when every argument is a pure value. A by-reference parameter writes
/// through the caller's lvalue, and neither the callee's signature nor a builtin's by-reference
/// shape is available at this point in the pass, so an lvalue argument is treated as a possible
/// out-parameter that could leave an object in this frame.
fn expr_binds_no_frame_storage(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::Variable(_)
        | ExprKind::This
        | ExprKind::ConstRef(_)
        | ExprKind::MagicConstant(_)
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => {
            named_local_increment_binds_no_frame_storage(name)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Clone(inner)
        | ExprKind::Throw(inner)
        | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::PropertyAccess { object: inner, .. }
        | ExprKind::NullsafePropertyAccess { object: inner, .. }
        | ExprKind::ObjectClassName { object: inner, .. } => expr_binds_no_frame_storage(inner),
        ExprKind::BinaryOp { left, right, .. } => {
            expr_binds_no_frame_storage(left) && expr_binds_no_frame_storage(right)
        }
        ExprKind::InstanceOf { value, target } => {
            expr_binds_no_frame_storage(value)
                && match target {
                    InstanceOfTarget::Name(_) => true,
                    InstanceOfTarget::Expr(target) => expr_binds_no_frame_storage(target),
                }
        }
        ExprKind::ArrayAccess { array, index } => {
            expr_binds_no_frame_storage(array) && expr_binds_no_frame_storage(index)
        }
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            expr_binds_no_frame_storage(value) && expr_binds_no_frame_storage(default)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_binds_no_frame_storage(condition)
                && expr_binds_no_frame_storage(then_expr)
                && expr_binds_no_frame_storage(else_expr)
        }
        ExprKind::ArrayLiteral(items) => items.iter().all(expr_binds_no_frame_storage),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .all(|(key, value)| {
                expr_binds_no_frame_storage(key) && expr_binds_no_frame_storage(value)
            }),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_binds_no_frame_storage(subject)
                && arms.iter().all(|(patterns, value)| {
                    patterns.iter().all(expr_binds_no_frame_storage)
                        && expr_binds_no_frame_storage(value)
                })
                && default.as_deref().is_none_or(expr_binds_no_frame_storage)
        }
        ExprKind::FunctionCall { name, args } => {
            !name
                .last_segment()
                .is_some_and(|segment| {
                    FRAME_WRITING_CALLS
                        .iter()
                        .any(|writer| segment.eq_ignore_ascii_case(writer))
                })
                && call_arguments_are_values(args)
        }
        ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. }
        | ExprKind::StaticMethodCall { args, .. } => call_arguments_are_values(args),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_binds_no_frame_storage(object) && call_arguments_are_values(args)
        }
        _ => false,
    }
}

/// Returns whether every argument in a call is a pure value rather than a possible out-parameter.
///
/// An lvalue argument can be bound to a by-reference parameter, and PHP then creates the name in
/// THIS frame when it did not exist and writes whatever the callee stores there. Since the proof
/// is about what the frame still owns at return, one such argument is enough to refuse it.
fn call_arguments_are_values(args: &[Expr]) -> bool {
    args.iter().all(|arg| {
        let value = match &arg.kind {
            ExprKind::NamedArg { value, .. } | ExprKind::Spread(value) => value.as_ref(),
            _ => arg,
        };
        !matches!(
            value.kind,
            ExprKind::Variable(_)
                | ExprKind::This
                | ExprKind::ArrayAccess { .. }
                | ExprKind::PropertyAccess { .. }
                | ExprKind::NullsafePropertyAccess { .. }
                | ExprKind::DynamicPropertyAccess { .. }
                | ExprKind::NullsafeDynamicPropertyAccess { .. }
                | ExprKind::StaticPropertyAccess { .. }
        ) && expr_binds_no_frame_storage(value)
    })
}

/// Returns whether named-local increment or decrement cannot bind destructible frame storage.
///
/// `PreIncrement` and friends store a variable name, not an l-value expression, so there is
/// no nested operand to walk. Property, static-property, and array-element increment is
/// desugared by the parser into assignment forms that [`expr_binds_no_frame_storage`] already
/// refuses. PHP 8 raises `TypeError` for `++` on an object or an array, so the named slot
/// stays a scalar.
fn named_local_increment_binds_no_frame_storage(_name: &str) -> bool {
    true
}

#[cfg(test)]
mod frame_binding_proof {
    use super::{expr_binds_no_frame_storage, frame_retires_nothing_destructible};
    use crate::parser::ast::{Stmt, StmtKind};

    /// Parses PHP source into a statement list.
    fn parse_stmts(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
        crate::parser::parse(&tokens).expect("fixture must parse")
    }

    /// Returns whether the first top-level function proves a scalar-only frame.
    fn first_function_frame_is_proven(source: &str) -> bool {
        let program = parse_stmts(source);
        match &program[0].kind {
            StmtKind::FunctionDecl {
                params,
                variadic,
                body,
                ..
            } => frame_retires_nothing_destructible(params, variadic.as_ref(), body),
            other => panic!("expected a function declaration, got {other:?}"),
        }
    }

    /// Verifies `++$value` of a typed scalar parameter still proves the frame clean.
    ///
    /// The AST names only that local. PHP 8 cannot leave an object in the slot, and the
    /// parser never stores a nested lvalue in `PreIncrement`.
    #[test]
    fn test_named_local_increment_proves_a_scalar_frame() {
        assert!(
            first_function_frame_is_proven(
                "<?php function bump(int $value): int { return ++$value; }"
            ),
            "++$value of an int parameter must remain a scalar-frame proof"
        );
        let program = parse_stmts("<?php echo $value++;");
        match &program[0].kind {
            StmtKind::Echo(expr) => assert!(
                expr_binds_no_frame_storage(expr),
                "postfix increment of a named local must not bind frame storage: {expr:?}"
            ),
            other => panic!("expected echo, got {other:?}"),
        }
    }

    /// Verifies increment of an array element whose index is an assignment refuses the proof.
    ///
    /// `$slots[$held = $value]++;` is parser-supported statement increment of a complex lvalue.
    /// The index assignment binds `$held` in this frame. Treating increment as a leaf that
    /// never binds would grant the scalar-frame proof and drop a reachable destructor catch.
    #[test]
    fn test_array_increment_with_index_assignment_refuses_the_scalar_frame_proof() {
        assert!(
            !first_function_frame_is_proven(
                "<?php function bump(int $value): int { $slots[$held = $value]++; return $value; }"
            ),
            "an assignment nested in an increment index must refuse the scalar-frame proof"
        );
    }

    /// Verifies expression-position property increment is not treated as a non-binding leaf.
    ///
    /// `$holder->n++` cannot be `PostIncrement(String)`; the parser desugars it to assignment.
    /// The proof must refuse that form rather than returning true without inspecting it.
    #[test]
    fn test_property_increment_refuses_the_no_frame_binding_proof() {
        let program = parse_stmts("<?php echo $holder->n++;");
        match &program[0].kind {
            StmtKind::Echo(expr) => assert!(
                !expr_binds_no_frame_storage(expr),
                "property increment must refuse the no-frame-binding proof: {expr:?}"
            ),
            other => panic!("expected echo, got {other:?}"),
        }
    }
}
