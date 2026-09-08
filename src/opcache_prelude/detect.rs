//! Purpose:
//! Decides, for a given prelude/profile-sensitive symbol name, whether a parsed program
//! references it — and WHERE — so the corresponding prelude function is injected only when
//! used, and whether the program already declares its own function of that name (so a user
//! definition is never clobbered).
//!
//! Called from:
//! - `crate::opcache_prelude::inject_if_used`.
//! - `crate::version_prelude::inject_if_used`.
//! - `crate::php_profile::sensitivity::scan`, which needs the SPAN of the first reference to
//!   point a diagnostic at the construct that made a program profile-dependent.
//!
//! Key details:
//! - The walk is generic over the target name so a single exhaustive traversal backs
//!   every prelude function; each function is detected and injected independently
//!   (pay-for-use per function, with its own redeclaration guard).
//! - Reference detection returns `Option<Span>` rather than `bool`: the span of the FIRST
//!   matching occurrence in traversal order. Short-circuiting is preserved exactly — `||`
//!   becomes `Option::or_else` and `Iterator::any` becomes `Iterator::find_map`, both of
//!   which stop at the first hit — so this is the same walk with a wider return type, not a
//!   more expensive one. `program_references` remains as a `bool` wrapper for the injection
//!   call sites that only need the yes/no answer.
//! - Runs before name resolution, so `Name`s are raw source text; matched on the
//!   unqualified last segment, since `\opcache_reset` and `opcache_reset` are the same
//!   symbol.
//! - The search is typed by [`SymbolKind`], because the kinds of symbol elephc needs to find
//!   obey DIFFERENT matching rules, and the right rule follows from what a false positive
//!   costs the caller. A `Function` is matched case-insensitively and also inside string
//!   literals, so `function_exists('opcache_reset')` and callable forms still inject the
//!   function; over-matching there only adds a small, later dead-code-eliminated declaration.
//!   A `Constant` is matched case-sensitively at `ConstRef` positions only, because its hits
//!   become user-facing diagnostics rather than dead bytes. A `CallSite` is a function
//!   matched at call positions only, for the minimum-version check, where a false positive
//!   costs a REJECTED COMPILE and where `function_exists` is a compatibility guard rather
//!   than evidence of a requirement. No two kinds ever cross-match.
//! - A `Function` search can be narrowed further by what the call ASKS ABOUT, through
//!   [`ArgFilter`]: `ini_get` is profile-dependent for `opcache.*` and not for anything else,
//!   and an `eval` fragment matters for what its source CONTAINS rather than what it equals.
//!   Every filter treats an unresolvable first argument as a match, since the compiler cannot
//!   read a string it does not have.
//! - Soundness (never missing a real use) is what matters, so the `match`es are exhaustive
//!   with no wildcard arm: a new `ExprKind`/`StmtKind` fails to compile here rather than
//!   silently becoming a blind spot.

use crate::names::Name;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    InstanceOfTarget, PackedField, Stmt, StmtKind, TraitUse, TypeExpr,
};
use crate::span::Span;

/// What kind of PHP symbol a reference walk is looking for.
///
/// The distinction is not cosmetic. PHP function names are CASE-INSENSITIVE and can be
/// reached through a string (`function_exists('opcache_reset')`, callable strings), whereas
/// global constant names are CASE-SENSITIVE and only ever appear at a `ConstRef` position.
/// Matching a constant case-insensitively would make a `$php_version_id`-ish spelling a hit;
/// matching one inside a string literal would make the word `PHP_VERSION_ID` appearing in a
/// help message or an error string a hit. Both would be false positives that no
/// dead-code-elimination pass can absorb, because a constant reference — unlike an
/// over-injected prelude function — is reported to the user as a diagnostic.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SymbolKind {
    /// A function name: matched case-insensitively, and also matched in string literals.
    Function,
    /// A function name matched case-insensitively at CALL POSITIONS ONLY — never inside a
    /// string literal.
    ///
    /// The difference from [`Self::Function`] is exactly `function_exists('json_validate')`,
    /// and which answer is right depends on what the caller does with it. A prelude injector
    /// wants the string to count, because the program may reach the function through it. A
    /// MINIMUM-VERSION check wants the opposite: `function_exists` around a name is the
    /// canonical way to guard a newer function so the code still runs on older PHP, so
    /// treating that guard as proof the program REQUIRES the newer version would reject the
    /// very idiom written to stay compatible.
    CallSite,
    /// A global constant name: matched case-sensitively, at `ConstRef` positions only.
    Constant,
    /// The PIPE OPERATOR (`|>`), matched as a syntactic form rather than by name.
    ///
    /// [`Symbol::name`] is ignored for this kind. The walk below is a "find the first node
    /// satisfying a predicate, exhaustively" search that happens to be used mostly for names;
    /// a syntactic form is the same search with a different predicate, and reusing it is what
    /// keeps elephc from carrying a SECOND exhaustive expression traversal that could drift
    /// out of step with this one. That risk is not hypothetical: the whole value of these
    /// `match`es is that they have no wildcard arm, and that guarantee is per-traversal.
    PipeOperator,
    /// A PROPERTY HOOK (`public string $x { get => ... }`), a PHP 8.4 form.
    PropertyHooks,
    /// ASYMMETRIC PROPERTY VISIBILITY (`public private(set) int $x`), a PHP 8.4 form.
    AsymmetricVisibility,
    /// A TYPED CLASS CONSTANT (`const string N = 'v'`), a PHP 8.3 form.
    TypedClassConst,
}

/// How a call's first argument narrows a FUNCTION match.
///
/// Every variant treats a first argument that is NOT a string literal — a variable, a
/// concatenation, anything computed — as a match, and a call with no arguments likewise:
/// the compiler cannot know what such a call will name at runtime, so the only safe answer
/// is that it might name a watched subject.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArgFilter<'a> {
    /// No narrowing: every call to the name matches.
    Any,
    /// Matches a string-literal first argument that STARTS WITH one of these.
    Prefixes(&'a [&'a str]),
    /// Matches a string-literal first argument that CONTAINS one of these.
    ///
    /// `eval` is the motivating case: the fragment is a PROGRAM, not a subject, so the
    /// question is whether the source it carries mentions a watched name anywhere, not
    /// whether the argument as a whole equals one.
    Substrings(&'a [&'a str]),
}

/// A named PHP symbol to search for, carrying the matching rules its kind implies.
///
/// Bundling the name and the kind into one `Copy` value keeps the walk's threading cost at a
/// single parameter, which is what lets the traversal below stay a mechanical forward of
/// `target` in every arm that has no matching position of its own.
#[derive(Clone, Copy)]
pub(crate) struct Symbol<'a> {
    /// The symbol name as written in PHP source.
    pub(crate) name: &'a str,
    /// Which matching rules apply — see [`SymbolKind`].
    pub(crate) kind: SymbolKind,
    /// Narrows a FUNCTION match by what the call asks about — see [`ArgFilter`].
    pub(crate) args: ArgFilter<'a>,
}

impl<'a> Symbol<'a> {
    /// A function symbol: case-insensitive, and string literals count as references.
    pub(crate) fn function(name: &'a str) -> Self {
        Self {
            name,
            kind: SymbolKind::Function,
            args: ArgFilter::Any,
        }
    }

    /// A function symbol narrowed by an arbitrary [`ArgFilter`].
    pub(crate) fn function_with_args(name: &'a str, args: ArgFilter<'a>) -> Self {
        Self {
            name,
            kind: SymbolKind::Function,
            args,
        }
    }

    /// A function symbol whose interesting behavior depends on WHICH SUBJECT it is asked
    /// about, identified by the prefix of its first argument.
    ///
    /// `ini_get` is the motivating case: `ini_get('opcache.jit')` reads a directive whose
    /// value moves with the compile profile, while `ini_get('precision')` reads one that does
    /// not (verified: both profiles return `bool(false)` for it). Matching `ini_get` by name
    /// alone would therefore make a true statement about the FUNCTION into a false statement
    /// about the PROGRAM.
    ///
    /// Matching is deliberately asymmetric, in the safe direction — see [`ArgFilter`] for the
    /// rule every filter shares.
    pub(crate) fn function_with_arg_prefixes(name: &'a str, prefixes: &'a [&'a str]) -> Self {
        Self::function_with_args(name, ArgFilter::Prefixes(prefixes))
    }

    /// A syntactic FORM rather than a name; [`Symbol::name`] is unused.
    ///
    /// Every syntactic search rides on this one traversal instead of getting its own. The
    /// value of these `match`es is that they have no wildcard arm, so a new `ExprKind` or
    /// `StmtKind` fails to compile rather than becoming a blind spot — and that guarantee is
    /// PER-TRAVERSAL. A second walk would double the surface on which it must hold and could
    /// drift out of step silently.
    pub(crate) fn syntactic(kind: SymbolKind) -> Self {
        Self {
            name: "",
            kind,
            args: ArgFilter::Any,
        }
    }

    /// A function symbol matched at CALL POSITIONS ONLY, never inside a string literal.
    ///
    /// See [`SymbolKind::CallSite`]: this is what a minimum-version check needs, so that a
    /// `function_exists()` guard is not mistaken for a hard requirement.
    pub(crate) fn call_site(name: &'a str) -> Self {
        Self {
            name,
            kind: SymbolKind::CallSite,
            args: ArgFilter::Any,
        }
    }

    /// A global constant symbol: case-sensitive, `ConstRef` positions only.
    pub(crate) fn constant(name: &'a str) -> Self {
        Self {
            name,
            kind: SymbolKind::Constant,
            args: ArgFilter::Any,
        }
    }
}

/// Returns whether a call's arguments can select one of `target`'s watched subjects.
///
/// Unconstrained symbols always match. See [`ArgFilter`] for why an unresolvable first
/// argument is treated as a match rather than a miss.
///
/// Matching is case-insensitive throughout, because every subject these filters name is
/// either a PHP function name or an ini directive, and neither is case-sensitive.
fn args_select_subject(args: &[Expr], target: Symbol<'_>) -> bool {
    let filter = match target.args {
        ArgFilter::Any => return true,
        other => other,
    };
    let Some(ExprKind::StringLiteral(value)) = args.first().map(|arg| &arg.kind) else {
        return true;
    };
    match filter {
        // `get` rather than a slice: a needle length can land inside a multi-byte character,
        // and a literal carrying one is a legal argument, not a compiler crash.
        ArgFilter::Prefixes(prefixes) => prefixes.iter().any(|prefix| {
            value
                .get(..prefix.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        }),
        ArgFilter::Substrings(substrings) => {
            let haystack = value.to_ascii_lowercase();
            substrings
                .iter()
                .any(|needle| haystack.contains(&needle.to_ascii_lowercase()))
        }
        ArgFilter::Any => true,
    }
}

/// Returns whether any top-level statement references `target` (a prelude FUNCTION name,
/// already lowercase), so that function must be injected ahead of user code.
///
/// This is the injection-side entry point and is deliberately function-only: every caller is
/// a prelude injector deciding whether to emit a function declaration.
pub(crate) fn program_references(program: &[Stmt], target: &str) -> bool {
    first_reference(program, Symbol::function(target)).is_some()
}

/// Returns the span of the FIRST reference to `target`, or `None` when the program never
/// mentions it.
///
/// Traversal order defines "first", which is source order for every construct the parser
/// builds in source order; callers use it only to point a diagnostic somewhere truthful
/// inside the program, never to reason about ordering between two different targets.
pub(crate) fn first_reference(program: &[Stmt], target: Symbol<'_>) -> Option<Span> {
    program.iter().find_map(|stmt| stmt_refs(stmt, target))
}

/// Returns whether the program already declares its own `target` function (at top level
/// or inside a namespace/guard/synthetic block), in which case that prelude function
/// must not be injected so the user definition wins and there is no redeclaration error.
pub(crate) fn program_declares(program: &[Stmt], target: &str) -> bool {
    program.iter().any(|stmt| stmt_declares(stmt, target))
}

/// Returns whether a CALL position names `target`, compared case-insensitively on its
/// unqualified last segment.
///
/// Always false for a [`SymbolKind::Constant`] target: a call position never names a
/// constant, so a constant search must not match `foo()` merely because a constant of that
/// name exists.
fn name_is(name: &Name, target: Symbol<'_>) -> bool {
    match target.kind {
        SymbolKind::Function | SymbolKind::CallSite => name
            .last_segment()
            .is_some_and(|segment| segment.eq_ignore_ascii_case(target.name)),
        SymbolKind::Constant
        | SymbolKind::PipeOperator
        | SymbolKind::PropertyHooks
        | SymbolKind::AsymmetricVisibility
        | SymbolKind::TypedClassConst => false,
    }
}

/// Returns whether a `ConstRef` position names `target`, compared CASE-SENSITIVELY on its
/// unqualified last segment (PHP global constants are case-sensitive; `\PHP_VERSION_ID` and
/// `PHP_VERSION_ID` are the same constant, which is why the last segment is what is compared).
///
/// Always false for a [`SymbolKind::Function`] target, so the existing prelude injectors keep
/// their exact previous behavior: a constant named `OPCACHE_RESET` does not cause the
/// `opcache_reset()` function to be injected.
fn const_name_is(name: &Name, target: Symbol<'_>) -> bool {
    match target.kind {
        SymbolKind::Constant => name
            .last_segment()
            .is_some_and(|segment| segment == target.name),
        SymbolKind::Function
        | SymbolKind::CallSite
        | SymbolKind::PipeOperator
        | SymbolKind::PropertyHooks
        | SymbolKind::AsymmetricVisibility
        | SymbolKind::TypedClassConst => false,
    }
}

/// Returns whether a statement declares a top-level `target` function, recursing only
/// into block forms that can host a hoisted declaration.
fn stmt_declares(stmt: &Stmt, target: &str) -> bool {
    match &stmt.kind {
        StmtKind::FunctionDecl { name, .. } => name.eq_ignore_ascii_case(target),
        StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => body.iter().any(|stmt| stmt_declares(stmt, target)),
        _ => false,
    }
}

/// Returns the span of a first-class-callable reference to `target` via a function name;
/// method/static-method targets cannot name it but their receiver is still walked for
/// nested references.
///
/// `CallableTarget::Function` carries no span of its own, so `enclosing` — the span of the
/// `FirstClassCallable` expression that owns it — is reported instead.
fn callable_target_refs(
    target_ref: &CallableTarget,
    target: Symbol<'_>,
    enclosing: Span,
) -> Option<Span> {
    match target_ref {
        CallableTarget::Function(name) => name_is(name, target).then_some(enclosing),
        CallableTarget::StaticMethod { .. } => None,
        CallableTarget::Method { object, .. } => expr_refs(object, target),
    }
}

/// Returns the span of the first parameter default value that references the function
/// (type hints cannot). Shared by function, method, and closure parameter lists.
fn params_ref(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    target: Symbol<'_>,
) -> Option<Span> {
    params
        .iter()
        .find_map(|(_, _, default, _)| default.as_ref().and_then(|expr| expr_refs(expr, target)))
}

/// Returns the span of a `use Trait` clause reference to the function; trait/method names
/// in adaptations are not call sites, so this is always `None`.
fn trait_use_refs(_trait_use: &TraitUse, _target: Symbol<'_>) -> Option<Span> {
    None
}

/// Returns the span of a class property default value that references the function.
fn class_property_refs(property: &ClassProperty, target: Symbol<'_>) -> Option<Span> {
    if target.kind == SymbolKind::PropertyHooks && property.hooks.any() {
        return Some(property.span);
    }
    if target.kind == SymbolKind::AsymmetricVisibility && property.set_visibility.is_some() {
        return Some(property.span);
    }
    property.default.as_ref().and_then(|expr| expr_refs(expr, target))
}

/// Returns the span of the first reference to the function in a method's parameter
/// defaults or body.
fn class_method_refs(method: &ClassMethod, target: Symbol<'_>) -> Option<Span> {
    params_ref(&method.params, target)
        .or_else(|| method.body.iter().find_map(|stmt| stmt_refs(stmt, target)))
}

/// Returns the span of a class constant initializer reference to the function.
fn class_const_refs(constant: &ClassConst, target: Symbol<'_>) -> Option<Span> {
    if target.kind == SymbolKind::TypedClassConst && constant.type_expr.is_some() {
        return Some(constant.span);
    }
    expr_refs(&constant.value, target)
}

/// Returns the span of an enum case backing-value reference to the function.
fn enum_case_refs(case: &EnumCaseDecl, target: Symbol<'_>) -> Option<Span> {
    case.value.as_ref().and_then(|expr| expr_refs(expr, target))
}

/// Returns the span of a `packed class` field reference to the function; packed fields
/// carry only types, never call sites, so this is always `None`.
fn packed_field_refs(_field: &PackedField, _target: Symbol<'_>) -> Option<Span> {
    None
}

/// Returns the span of an `instanceof` target's runtime-expression operand reference to
/// the function (name targets are class positions, never call sites).
fn instanceof_target_refs(target_ref: &InstanceOfTarget, target: Symbol<'_>) -> Option<Span> {
    match target_ref {
        InstanceOfTarget::Name(_) => None,
        InstanceOfTarget::Expr(expr) => expr_refs(expr, target),
    }
}

/// Returns the span of the first reference to `target` in an expression, at any call
/// position or as a matching string literal, recursing into every child. The `match` is
/// exhaustive so a new `ExprKind` cannot silently bypass detection.
fn expr_refs(expr: &Expr, target: Symbol<'_>) -> Option<Span> {
    match &expr.kind {
        // `require`/`include` in expression position: recurse into the path expression.
        ExprKind::IncludeValue { path, .. } => expr_refs(path, target),
        // A matching string literal counts (function_exists/callable) — for FUNCTION targets
        // only. A constant name inside a string is prose, not a reference to the constant.
        // A matching string is a reference for `Function` (it may be how the program reaches
        // the callee) but NOT for `CallSite` — see [`SymbolKind::CallSite`].
        ExprKind::StringLiteral(value) => (target.kind == SymbolKind::Function
            && value.eq_ignore_ascii_case(target.name))
        .then_some(expr.span),

        // The only position at which a global constant is referenced.
        ExprKind::ConstRef(name) => const_name_is(name, target).then_some(expr.span),

        // Leaves and identifier-only forms carry no call site.
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::This
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::MagicConstant(_) => None,

        ExprKind::FunctionCall { name, args } => (name_is(name, target)
            && args_select_subject(args, target))
        .then_some(expr.span)
        .or_else(|| args.iter().find_map(|arg| expr_refs(arg, target))),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => expr_refs(object, target)
            .or_else(|| args.iter().find_map(|arg| expr_refs(arg, target))),
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => expr_refs(object, target)
            .or_else(|| expr_refs(method, target))
            .or_else(|| args.iter().find_map(|arg| expr_refs(arg, target))),
        ExprKind::StaticMethodCall { args, .. } => {
            args.iter().find_map(|arg| expr_refs(arg, target))
        }
        ExprKind::FirstClassCallable(callable) => {
            callable_target_refs(callable, target, expr.span)
        }

        ExprKind::BinaryOp { left, right, .. } => {
            expr_refs(left, target).or_else(|| expr_refs(right, target))
        }
        ExprKind::InstanceOf { value, target: iof } => {
            expr_refs(value, target).or_else(|| instanceof_target_refs(iof, target))
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::YieldFrom(inner) => expr_refs(inner, target),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_refs(value, target).or_else(|| expr_refs(default, target))
        }
        ExprKind::Pipe { value, callable } => (target.kind == SymbolKind::PipeOperator)
            .then_some(expr.span)
            .or_else(|| expr_refs(value, target))
            .or_else(|| expr_refs(callable, target)),
        ExprKind::Assignment {
            target: assign_target,
            value,
            result_target,
            prelude,
            ..
        } => expr_refs(assign_target, target)
            .or_else(|| expr_refs(value, target))
            .or_else(|| {
                result_target
                    .as_deref()
                    .and_then(|expr| expr_refs(expr, target))
            })
            .or_else(|| prelude.iter().find_map(|stmt| stmt_refs(stmt, target))),
        ExprKind::ClosureCall { args, .. } => args.iter().find_map(|arg| expr_refs(arg, target)),
        ExprKind::ArrayLiteral(items) => items.iter().find_map(|item| expr_refs(item, target)),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs.iter().find_map(|(key, value)| {
            expr_refs(key, target).or_else(|| expr_refs(value, target))
        }),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => expr_refs(subject, target)
            .or_else(|| {
                arms.iter().find_map(|(conditions, body)| {
                    conditions
                        .iter()
                        .find_map(|cond| expr_refs(cond, target))
                        .or_else(|| expr_refs(body, target))
                })
            })
            .or_else(|| default.as_deref().and_then(|expr| expr_refs(expr, target))),
        ExprKind::ArrayAccess { array, index } => {
            expr_refs(array, target).or_else(|| expr_refs(index, target))
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => expr_refs(condition, target)
            .or_else(|| expr_refs(then_expr, target))
            .or_else(|| expr_refs(else_expr, target)),
        ExprKind::Cast { expr, .. } | ExprKind::PtrCast { expr, .. } => expr_refs(expr, target),
        ExprKind::Closure { params, body, .. } => params_ref(params, target)
            .or_else(|| body.iter().find_map(|stmt| stmt_refs(stmt, target))),
        ExprKind::NamedArg { value, .. } => expr_refs(value, target),
        ExprKind::ExprCall { callee, args } => expr_refs(callee, target)
            .or_else(|| args.iter().find_map(|arg| expr_refs(arg, target))),
        ExprKind::NewObject { args, .. } => args.iter().find_map(|arg| expr_refs(arg, target)),
        ExprKind::NewDynamic { name_expr, args } => expr_refs(name_expr, target)
            .or_else(|| args.iter().find_map(|arg| expr_refs(arg, target))),
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => expr_refs(class_name, target)
            .or_else(|| args.iter().find_map(|arg| expr_refs(arg, target))),
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_refs(object, target),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_refs(object, target).or_else(|| expr_refs(property, target))
        }
        ExprKind::StaticPropertyAccess { .. } => None,
        ExprKind::BufferNew { len, .. } => expr_refs(len, target),
        ExprKind::ClassConstant { .. } | ExprKind::ScopedConstantAccess { .. } => None,
        ExprKind::ObjectClassName { object } => expr_refs(object, target),
        ExprKind::NewScopedObject { args, .. } => {
            args.iter().find_map(|arg| expr_refs(arg, target))
        }
        ExprKind::Yield { key, value } => key
            .as_deref()
            .and_then(|expr| expr_refs(expr, target))
            .or_else(|| value.as_deref().and_then(|expr| expr_refs(expr, target))),
    }
}

/// Returns the span of the first reference to `target` in a statement, at any call
/// position or string literal, recursing into nested statements, expressions, and class
/// members. The `match` is exhaustive so a new `StmtKind` cannot silently bypass
/// detection.
fn stmt_refs(stmt: &Stmt, target: Symbol<'_>) -> Option<Span> {
    match &stmt.kind {
        // Statements with no call position and no child expr/stmt.
        StmtKind::RefAssign { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::Global { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => None,

        StmtKind::Echo(expr) | StmtKind::Throw(expr) | StmtKind::ExprStmt(expr) => {
            expr_refs(expr, target)
        }
        StmtKind::Assign { value, .. } => expr_refs(value, target),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => expr_refs(condition, target)
            .or_else(|| then_body.iter().find_map(|stmt| stmt_refs(stmt, target)))
            .or_else(|| {
                elseif_clauses.iter().find_map(|(cond, body)| {
                    expr_refs(cond, target)
                        .or_else(|| body.iter().find_map(|stmt| stmt_refs(stmt, target)))
                })
            })
            .or_else(|| {
                else_body
                    .as_ref()
                    .and_then(|body| body.iter().find_map(|stmt| stmt_refs(stmt, target)))
            }),
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => then_body
            .iter()
            .find_map(|stmt| stmt_refs(stmt, target))
            .or_else(|| {
                else_body
                    .as_ref()
                    .and_then(|body| body.iter().find_map(|stmt| stmt_refs(stmt, target)))
            }),
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_refs(condition, target)
                .or_else(|| body.iter().find_map(|stmt| stmt_refs(stmt, target)))
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => init
            .as_deref()
            .and_then(|stmt| stmt_refs(stmt, target))
            .or_else(|| condition.as_ref().and_then(|expr| expr_refs(expr, target)))
            .or_else(|| update.as_deref().and_then(|stmt| stmt_refs(stmt, target)))
            .or_else(|| body.iter().find_map(|stmt| stmt_refs(stmt, target))),
        StmtKind::ArrayAssign { index, value, .. } => {
            expr_refs(index, target).or_else(|| expr_refs(value, target))
        }
        StmtKind::NestedArrayAssign { target: t, value } => {
            expr_refs(t, target).or_else(|| expr_refs(value, target))
        }
        StmtKind::ArrayPush { value, .. } => expr_refs(value, target),
        StmtKind::TypedAssign { value, .. } => expr_refs(value, target),
        StmtKind::Foreach { array, body, .. } => expr_refs(array, target)
            .or_else(|| body.iter().find_map(|stmt| stmt_refs(stmt, target))),
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => expr_refs(subject, target)
            .or_else(|| {
                cases.iter().find_map(|(conditions, body)| {
                    conditions
                        .iter()
                        .find_map(|cond| expr_refs(cond, target))
                        .or_else(|| body.iter().find_map(|stmt| stmt_refs(stmt, target)))
                })
            })
            .or_else(|| {
                default
                    .as_ref()
                    .and_then(|body| body.iter().find_map(|stmt| stmt_refs(stmt, target)))
            }),
        StmtKind::Include { path, .. } => expr_refs(path, target),
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => {
            body.iter().find_map(|stmt| stmt_refs(stmt, target))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => try_body
            .iter()
            .find_map(|stmt| stmt_refs(stmt, target))
            .or_else(|| {
                catches.iter().find_map(|catch| {
                    catch.body.iter().find_map(|stmt| stmt_refs(stmt, target))
                })
            })
            .or_else(|| {
                finally_body
                    .as_ref()
                    .and_then(|body| body.iter().find_map(|stmt| stmt_refs(stmt, target)))
            }),
        StmtKind::FunctionDecl { params, body, .. } => params_ref(params, target)
            .or_else(|| body.iter().find_map(|stmt| stmt_refs(stmt, target))),
        StmtKind::Return(value) => value.as_ref().and_then(|expr| expr_refs(expr, target)),
        StmtKind::ConstDecl { value, .. } => expr_refs(value, target),
        StmtKind::ListUnpack { value, .. } => expr_refs(value, target),
        StmtKind::StaticVar { init, .. } => expr_refs(init, target),
        StmtKind::ClassDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => trait_uses
            .iter()
            .find_map(|tu| trait_use_refs(tu, target))
            .or_else(|| properties.iter().find_map(|p| class_property_refs(p, target)))
            .or_else(|| methods.iter().find_map(|m| class_method_refs(m, target)))
            .or_else(|| constants.iter().find_map(|c| class_const_refs(c, target))),
        StmtKind::EnumDecl { cases, .. } => {
            cases.iter().find_map(|case| enum_case_refs(case, target))
        }
        StmtKind::PackedClassDecl { fields, .. } => {
            fields.iter().find_map(|f| packed_field_refs(f, target))
        }
        StmtKind::InterfaceDecl {
            properties,
            methods,
            constants,
            ..
        } => properties
            .iter()
            .find_map(|p| class_property_refs(p, target))
            .or_else(|| methods.iter().find_map(|m| class_method_refs(m, target)))
            .or_else(|| constants.iter().find_map(|c| class_const_refs(c, target))),
        StmtKind::TraitDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => trait_uses
            .iter()
            .find_map(|tu| trait_use_refs(tu, target))
            .or_else(|| properties.iter().find_map(|p| class_property_refs(p, target)))
            .or_else(|| methods.iter().find_map(|m| class_method_refs(m, target)))
            .or_else(|| constants.iter().find_map(|c| class_const_refs(c, target))),
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_refs(object, target).or_else(|| expr_refs(value, target))
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => expr_refs(value, target),
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            expr_refs(index, target).or_else(|| expr_refs(value, target))
        }
        StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_refs(object, target).or_else(|| expr_refs(value, target))
        }
        StmtKind::DynamicPropertyArrayPush {
            object,
            property,
            value,
        } => expr_refs(object, target)
            .or_else(|| expr_refs(property, target))
            .or_else(|| expr_refs(value, target)),
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => expr_refs(object, target)
            .or_else(|| expr_refs(index, target))
            .or_else(|| expr_refs(value, target)),
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the generic usage AST walk over both OPcache prelude function
    //! names: a procedural call, a string reference (function_exists/callable), and a
    //! nested reference are detected; an unrelated program is not; and a user
    //! declaration is recognized. Exercised for `opcache_get_configuration` and
    //! `opcache_reset`.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - These tests drive `program_references`, which is a thin `is_some()` wrapper over
    //!   `first_reference`, so they cover the span-returning walk too. `first_reference`
    //!   additionally has span-specific tests below.

    use super::*;

    /// The two OPcache prelude function names, exercised by every reference test.
    const GET_CONFIGURATION: &str = "opcache_get_configuration";
    const RESET: &str = "opcache_reset";

    /// Parses source the way `inject_if_used` sees it: tokenize then parse.
    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// A procedural `opcache_get_configuration()` call is detected.
    #[test]
    fn detects_procedural_call() {
        assert!(program_references(
            &parse(r#"<?php $c = opcache_get_configuration();"#),
            GET_CONFIGURATION
        ));
    }

    /// A procedural `opcache_reset()` call is detected.
    #[test]
    fn detects_reset_call() {
        assert!(program_references(
            &parse(r#"<?php var_dump(opcache_reset());"#),
            RESET
        ));
    }

    /// An `"opcache_reset"` string (function_exists/callable) is detected.
    #[test]
    fn detects_reset_string_reference() {
        assert!(program_references(
            &parse(r#"<?php if (function_exists("opcache_reset")) { echo "y"; }"#),
            RESET
        ));
    }

    /// A nested reference inside a function body is detected.
    #[test]
    fn detects_nested_reference() {
        assert!(program_references(
            &parse(r#"<?php function f() { return opcache_reset(); }"#),
            RESET
        ));
    }

    /// Case-insensitive matching, as PHP function names are.
    #[test]
    fn detects_case_insensitive() {
        assert!(program_references(&parse(r#"<?php OPCACHE_RESET();"#), RESET));
    }

    /// Detection is per-name: a program using only `opcache_reset` does not count as
    /// referencing `opcache_get_configuration`.
    #[test]
    fn reference_detection_is_per_name() {
        let program = parse(r#"<?php opcache_reset();"#);
        assert!(program_references(&program, RESET));
        assert!(!program_references(&program, GET_CONFIGURATION));
    }

    /// A program with no reference is not detected.
    #[test]
    fn ignores_unrelated_program() {
        let program = parse(r#"<?php $a = [1, 2]; echo count($a);"#);
        assert!(!program_references(&program, GET_CONFIGURATION));
        assert!(!program_references(&program, RESET));
    }

    /// A user-declared function is recognized so the prelude is skipped, per name.
    #[test]
    fn detects_user_declaration() {
        let program = parse(r#"<?php function opcache_reset(): bool { return true; }"#);
        assert!(program_declares(&program, RESET));
        assert!(!program_declares(&program, GET_CONFIGURATION));
    }

    /// A program that only calls it does not count as declaring it.
    #[test]
    fn call_is_not_a_declaration() {
        assert!(!program_declares(
            &parse(r#"<?php opcache_reset();"#),
            RESET
        ));
    }

    /// `first_reference` reports the line of the reference, not of the program start,
    /// which is what makes it usable as a diagnostic anchor.
    #[test]
    fn first_reference_reports_the_reference_line() {
        let program = parse("<?php\n$a = 1;\n$b = 2;\nopcache_reset();\n");
        let span = first_reference(&program, Symbol::function(RESET)).expect("reference must be found");
        assert_eq!(span.line, 4);
    }

    /// `first_reference` reports the FIRST occurrence when several exist, so the
    /// diagnostic points at the earliest place the program became dependent.
    #[test]
    fn first_reference_reports_the_earliest_occurrence() {
        let program = parse("<?php\nopcache_reset();\n$x = 1;\nopcache_reset();\n");
        let span = first_reference(&program, Symbol::function(RESET)).expect("reference must be found");
        assert_eq!(span.line, 2);
    }

    /// `first_reference` reaches inside a nested body and reports that inner line rather
    /// than the enclosing declaration's line.
    #[test]
    fn first_reference_descends_into_nested_bodies() {
        let program = parse("<?php\nfunction f() {\n    return opcache_reset();\n}\n");
        let span = first_reference(&program, Symbol::function(RESET)).expect("reference must be found");
        assert_eq!(span.line, 3);
    }

    /// An absent name yields `None`, the condition `program_references` reports as false.
    #[test]
    fn first_reference_is_none_when_absent() {
        assert!(first_reference(&parse(r#"<?php echo 1;"#), Symbol::function(RESET)).is_none());
    }

    /// A bare global constant reference is found, which the function-only walk could not
    /// see: `ConstRef` used to sit in the "carries no call site" arm, making
    /// `PHP_VERSION_ID` — the canonical profile-dependent construct — invisible.
    #[test]
    fn constant_reference_is_detected() {
        let program = parse("<?php\n$a = 1;\nif (PHP_VERSION_ID >= 80400) { echo 'new'; }\n");
        let span = first_reference(&program, Symbol::constant("PHP_VERSION_ID"))
            .expect("constant reference must be found");
        assert_eq!(span.line, 3);
    }

    /// Constant matching is CASE-SENSITIVE, as PHP global constants are.
    #[test]
    fn constant_matching_is_case_sensitive() {
        let program = parse(r#"<?php echo php_version_id;"#);
        assert!(first_reference(&program, Symbol::constant("PHP_VERSION_ID")).is_none());
    }

    /// A constant name appearing inside a STRING is prose, not a reference. This is the
    /// difference that makes the constant kind worth having: the function kind matches
    /// strings on purpose (`function_exists`), and reusing it for constants would flag
    /// every help message that happens to name one.
    #[test]
    fn constant_in_a_string_is_not_a_reference() {
        let program = parse(r#"<?php echo "requires PHP_VERSION_ID >= 80400";"#);
        assert!(first_reference(&program, Symbol::constant("PHP_VERSION_ID")).is_none());
    }

    /// A constant search never matches a CALL of the same name, and a function search never
    /// matches a CONSTANT of the same name. This is what keeps the existing prelude
    /// injectors byte-identical in behavior after the kind split.
    #[test]
    fn the_two_kinds_do_not_cross_match() {
        let call = parse(r#"<?php opcache_reset();"#);
        assert!(first_reference(&call, Symbol::constant("opcache_reset")).is_none());

        let constant = parse(r#"<?php echo OPCACHE_RESET;"#);
        assert!(first_reference(&constant, Symbol::function("opcache_reset")).is_none());
    }

    /// A namespaced/leading-backslash spelling resolves to the same global constant.
    #[test]
    fn leading_backslash_constant_is_detected() {
        let program = parse(r#"<?php echo \PHP_VERSION_ID;"#);
        assert!(first_reference(&program, Symbol::constant("PHP_VERSION_ID")).is_some());
    }

    /// A `CallSite` symbol matches a real call, like `Function` does.
    #[test]
    fn call_site_matches_a_call() {
        let program = parse(r#"<?php var_dump(json_validate("{}"));"#);
        assert!(first_reference(&program, Symbol::call_site("json_validate")).is_some());
    }

    /// A `CallSite` symbol does NOT match a string mention, where `Function` does. This is
    /// the whole reason the kind exists: `function_exists('json_validate')` is a
    /// compatibility guard, not evidence that the program requires the function.
    #[test]
    fn call_site_ignores_string_mentions_where_function_matches() {
        let program = parse(r#"<?php if (function_exists("json_validate")) { echo "y"; }"#);
        assert!(first_reference(&program, Symbol::function("json_validate")).is_some());
        assert!(first_reference(&program, Symbol::call_site("json_validate")).is_none());
    }

    /// `CallSite` keeps `Function`'s case-insensitivity, as PHP function names are.
    #[test]
    fn call_site_is_case_insensitive() {
        let program = parse(r#"<?php JSON_VALIDATE("{}");"#);
        assert!(first_reference(&program, Symbol::call_site("json_validate")).is_some());
    }

    /// `CallSite` never matches a constant of the same name.
    #[test]
    fn call_site_does_not_match_a_constant() {
        let program = parse(r#"<?php echo JSON_VALIDATE;"#);
        assert!(first_reference(&program, Symbol::call_site("json_validate")).is_none());
    }

    /// Argument narrowing selects the named subject, which is how a feature-detection guard
    /// for one function is told apart from a guard for another.
    #[test]
    fn arg_prefix_narrowing_selects_the_named_subject() {
        let program = parse(r#"<?php if (function_exists("array_find")) { echo "y"; }"#);
        assert!(first_reference(
            &program,
            Symbol::function_with_arg_prefixes("function_exists", &["array_find"])
        )
        .is_some());
        assert!(first_reference(
            &program,
            Symbol::function_with_arg_prefixes("function_exists", &["json_validate"])
        )
        .is_none());
    }

    /// An argument the compiler cannot resolve to a literal matches unconditionally, which is
    /// the safe direction for a caller that must not miss a real use.
    #[test]
    fn arg_prefix_narrowing_matches_unresolvable_arguments() {
        let program = parse(r#"<?php if (function_exists($name)) { echo "y"; }"#);
        assert!(first_reference(
            &program,
            Symbol::function_with_arg_prefixes("function_exists", &["json_validate"])
        )
        .is_some());
    }

    /// An UNFILTERED function symbol ignores arguments entirely, whatever they are.
    ///
    /// This is the property every prelude injector depends on — they all search by bare name —
    /// and it is what makes argument narrowing an opt-in rather than a change of meaning for
    /// the existing callers. Pinning it here is what keeps a future [`ArgFilter`] variant from
    /// quietly acquiring a default that filters.
    #[test]
    fn an_unfiltered_symbol_ignores_arguments() {
        for source in [
            r#"<?php opcache_reset();"#,
            r#"<?php opcache_reset("anything");"#,
            r#"<?php opcache_reset($computed);"#,
        ] {
            let program = parse(source);
            assert!(
                first_reference(&program, Symbol::function("opcache_reset")).is_some(),
                "an unfiltered symbol missed `{source}`"
            );
        }
    }

    /// A needle longer than the literal, or landing inside a multi-byte character, is a MISS
    /// rather than a panic.
    ///
    /// `ini_get("précision")` is legal PHP. Slicing the literal at the needle's byte length
    /// would split `é` and abort the compiler, so the match goes through `str::get`. A crash
    /// here would be a compiler bug reachable from ordinary source, not a detection error.
    #[test]
    fn prefix_narrowing_survives_a_multibyte_literal() {
        let program = parse(r#"<?php ini_get("précision"); ini_get("x");"#);
        assert!(first_reference(
            &program,
            Symbol::function_with_arg_prefixes("ini_get", &["opcache."])
        )
        .is_none());
    }

    /// Substring narrowing looks anywhere in the literal and ignores case, so an `eval`
    /// fragment is matched on what its source says rather than on how it is spelled.
    #[test]
    fn substring_narrowing_matches_anywhere_and_ignores_case() {
        let matching = parse(r#"<?php eval('echo php_version_id;');"#);
        assert!(first_reference(
            &matching,
            Symbol::function_with_args("eval", ArgFilter::Substrings(&["PHP_VERSION"]))
        )
        .is_some());

        let unrelated = parse(r#"<?php eval('echo 1 + 1;');"#);
        assert!(first_reference(
            &unrelated,
            Symbol::function_with_args("eval", ArgFilter::Substrings(&["PHP_VERSION"]))
        )
        .is_none());
    }
}
