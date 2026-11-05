//! Purpose:
//! Decides whether a parsed program references PHP's `ext/tidy` surface — any
//! `tidy_*` function, the `tidy` / `tidyNode` classes, or the `ob_tidyhandler`
//! output callback — so the tidy prelude is injected only for programs that use it.
//!
//! Called from:
//! - `crate::tidy_prelude::inject_if_used`.
//!
//! Key details:
//! - Runs before the pipeline's name-resolution phase, so [`program_uses_tidy`] first resolves
//!   a clone with the exact global tidy function/class surface seeded as fallback symbols.
//!   This applies PHP's namespace/import rules without injecting the prelude:
//!   `namespace App; tidy_parse_string()` falls back to the global function, while a
//!   namespace-local declaration or an imported user function remains unrelated.
//! - FOUR KINDS OF POSITION trigger injection: (a) a function call naming one of the
//!   24 `tidy_*` functions or `ob_tidyhandler`; (b) a class-name position naming `tidy`
//!   or `tidyNode` (new, static receivers, `instanceof`, type hints, trait uses, imports);
//!   (c) a string literal naming `ob_tidyhandler`, a tidy function, or the distinctive
//!   `tidyNode` class, which can become a runtime callable or class operand; (d) the
//!   source string of a literal `eval()`, scanned on identifier boundaries with class
//!   names accepted only in class positions. A bare `'tidy'` string is ordinary data and
//!   is not a trigger. `TIDY_*` constants are NOT a trigger either: they resolve from the
//!   shared constant catalog whether or not the prelude is injected.
//! - THE FUNCTION SET IS EXACT (the shared catalog's `Area::Tidy` names plus
//!   `ob_tidyhandler`), never a prefix, so a user `tidy_helper()` does not pull in the bridge.
//! - A CLASS NAME THE RESOLVER LEAVES BARE still counts. The resolver rewrites the class
//!   names it visits to the seeded global form, but it copies a variadic parameter's
//!   element type verbatim and resolves `X::class` receivers without the seeded table, so
//!   `tidyNode ...$nodes` and `tidyNode::class` stay the bare spelling; matching that
//!   spelling (one segment, any case) keeps those positions sound. Inside a namespace the
//!   bare spelling may over-inject for a same-named local class — the accepted trade-off.
//! - Soundness over precision: a missed reference would drop the prelude and turn a valid
//!   program into an "undefined function/class" error, so the `match`es are exhaustive (no
//!   wildcard arm). Adding an AST node forces this file to be updated. Opaque eval
//!   source, and a runtime-assembled dynamic class name for `tidy` or `tidyNode`, reach
//!   the surface through `--with-tidy`, exactly like the other bridge-backed preludes.

use crate::names::Name;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    InstanceOfTarget, PackedField, StaticReceiver, Stmt, StmtKind, TraitAdaptation, TraitUse,
    TypeExpr,
};

/// The exact PHP functions of the tidy surface: every `Area::Tidy` contract in the shared
/// catalog. Whole-segment matches ensure user helpers with an `tidy_` prefix do not silently
/// opt into the bridge.
fn tidy_functions() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        elephc_builtin_contract::contracts()
            .iter()
            .filter(|contract| {
                contract.area == elephc_builtin_contract::Area::Tidy && !contract.internal
            })
            .map(|contract| contract.name)
            .chain(std::iter::once("ob_tidyhandler"))
            .collect()
    })
}

/// The `tidyNode` and `tidyNode` classes, from the shared class catalog.
fn tidy_classes() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        crate::types::builtin_classes::class_names_in_module(
            elephc_builtin_contract::PhpModule::Tidy,
        )
        .to_vec()
    })
}

/// Returns whether any statement resolves to the global tidy surface, so the prelude must
/// be injected ahead of user code. Resolution errors are left for the pipeline's normal
/// name-resolution phase; an invalid program does not need speculative bridge injection.
pub(super) fn program_uses_tidy(program: &[Stmt]) -> bool {
    crate::name_resolver::resolve_with_additional_global_symbols(
        program.to_vec(),
        tidy_functions(),
        tidy_classes(),
    )
    .is_ok_and(|resolved| resolved.iter().any(stmt_refs_tidy))
}


/// Returns true when `name` is the language's `eval` function at this pre-resolution point.
fn is_eval_call(name: &Name) -> bool {
    matches!(name.parts.as_slice(), [candidate] if candidate.eq_ignore_ascii_case("eval"))
}

/// Scans a literal `eval()` fragment for tidy identifiers without parsing it.
///
/// The compiler deliberately does not parse runtime source outside its own frontend; this is a
/// token-boundary scan, so a fragment that mentions a tidy function, class, or
/// `ob_tidyhandler` pulls the prelude in even when the outer program never names the surface.
/// Over-injection is safe: a fragment that merely names a `$tidy` variable carries declarations
/// nothing calls.
fn eval_source_refs_tidy(expr: &Expr) -> bool {
    let ExprKind::StringLiteral(source) = &expr.kind else {
        return false;
    };
    let token_refs_tidy = |start: usize, end: usize| {
        let token = &source[start..end];
        let prefix = &source[..start];
        // A variable (`$tidy`) or a member name (`->tidy`, `::tidy`) is not a tidy
        // function/class reference even though it shares the identifier shape.
        if prefix.ends_with('$') || prefix.ends_with("->") || prefix.ends_with("::") {
            return false;
        }
        if tidy_functions()
            .iter()
            .any(|name| token.eq_ignore_ascii_case(name))
        {
            return true;
        }
        tidy_classes()
            .iter()
            .any(|name| token.eq_ignore_ascii_case(name))
            && eval_class_token_is_a_class_position(source, start, end)
    };
    let mut start = None;
    for (index, character) in source.char_indices() {
        if character.is_ascii_alphanumeric() || character == '_' {
            start.get_or_insert(index);
            continue;
        }
        if let Some(token_start) = start.take() {
            if token_refs_tidy(token_start, index) {
                return true;
            }
        }
    }
    start.is_some_and(|token_start| token_refs_tidy(token_start, source.len()))
}

/// Returns whether a `tidy`/`tidyNode` token appears where an eval fragment can name a class.
///
/// The scan cannot parse the fragment, so it recognises the surrounding syntactic anchors that
/// matter: a `::` receiver, a type position (`(`, `:`, `?`, `|`, `&`, `<`, `,`), or a class-ish
/// keyword/modifier. A quoted `'tidy'`, a comment, a variable, and a member name therefore stay
/// ordinary data unless the surrounding source really makes them class references.
fn eval_class_token_is_a_class_position(source: &str, start: usize, end: usize) -> bool {
    if source[end..].trim_start().starts_with("::") {
        return true;
    }
    let before = source[..start].trim_end();
    if before.ends_with([':', '?', '|', '&', '<', ',', '(']) {
        return true;
    }
    let before = before.trim_end_matches('\\').trim_end();
    let previous = before
        .rsplit(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .next()
        .unwrap_or("");
    matches!(
        previous,
        "new"
            | "instanceof"
            | "extends"
            | "implements"
            | "use"
            | "namespace"
            | "class"
            | "interface"
            | "trait"
            | "enum"
            | "function"
            | "fn"
            | "public"
            | "private"
            | "protected"
            | "static"
            | "readonly"
            | "var"
    )
}

/// Returns whether a resolved name is one of the tidy global functions. Function names are
/// case-insensitive, but a name with namespace segments is unrelated to the global surface.
fn name_is_tidy_function(name: &Name) -> bool {
    tidy_functions()
        .iter()
        .any(|candidate| crate::name_resolver::is_additional_global_symbol(name, candidate))
}

/// Returns whether a resolved name is `tidyNode` or `tidyNode`: the seeded global the
/// resolver bound it to, or the bare single-segment spelling at a class-name position the
/// resolver leaves as written (a variadic parameter's element type, a `::class` or scoped
/// constant receiver). Class names are case-insensitive, but a name with namespace segments
/// is unrelated to the global surface.
fn name_is_tidy_class(name: &Name) -> bool {
    tidy_classes().iter().any(|candidate| {
        crate::name_resolver::is_additional_global_symbol(name, candidate)
            || matches!(name.parts.as_slice(), [bare] if bare.eq_ignore_ascii_case(candidate))
    })
}

/// Returns whether a static receiver names a tidy class (`tidy::repairString()`).
/// `self`, `static`, and `parent` never resolve to a tidy class at this position.
fn receiver_refs_tidy(receiver: &StaticReceiver) -> bool {
    matches!(receiver, StaticReceiver::Named(name) if name_is_tidy_class(name))
}

/// Returns whether an `instanceof` target references a tidy class, recursing into the
/// operand when the target is a runtime expression.
fn instanceof_target_refs_tidy(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(name) => name_is_tidy_class(name),
        InstanceOfTarget::Generic(class_type) => type_refs_tidy(class_type),
        InstanceOfTarget::Expr(expr) => expr_refs_tidy(expr),
    }
}

/// Returns whether a first-class-callable target references the tidy surface: a free
/// function (`tidy_parse_string(...)`), a static-method receiver, or an instance-method object
/// expression.
fn callable_target_refs_tidy(target: &CallableTarget) -> bool {
    match target {
        CallableTarget::Function(name) => name_is_tidy_function(name),
        CallableTarget::StaticMethod { receiver, .. } => receiver_refs_tidy(receiver),
        CallableTarget::Method { object, .. } => expr_refs_tidy(object),
    }
}

/// Returns whether a type expression names a tidy class, recursing through
/// nullable/union/array/buffer wrappers and `ptr<Class>` targets.
fn type_refs_tidy(type_expr: &TypeExpr) -> bool {
    match type_expr {
        TypeExpr::Int
        | TypeExpr::Float
        | TypeExpr::Bool
        | TypeExpr::False
        | TypeExpr::Str
        | TypeExpr::Void
        | TypeExpr::Never
        | TypeExpr::Iterable => false,
        TypeExpr::Ptr(target) => target.as_ref().is_some_and(|name| name_is_tidy_class(&crate::names::pointer_target_leaf_name(name))),
        TypeExpr::Array(inner) | TypeExpr::Buffer(inner) | TypeExpr::Nullable(inner) => {
            type_refs_tidy(inner)
        }
        TypeExpr::AssocArray { key, value } => type_refs_tidy(key) || type_refs_tidy(value),
        // A declared signature can name one of these classes, and a program
        // that mentions it only through a callback still needs the prelude.
        TypeExpr::CallableSig { params, ret } => {
            params.iter().any(type_refs_tidy) || type_refs_tidy(ret)
        }
        TypeExpr::Named(name) => name_is_tidy_class(name),
        TypeExpr::GenericClass { name, args } => {
            name_is_tidy_class(name) || args.iter().any(type_refs_tidy)
        }
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            members.iter().any(type_refs_tidy)
        }
    }
}

/// Returns whether any parameter's type hint or default value references the tidy
/// surface. Shared by function, method, and closure parameter lists.
fn params_ref_tidy(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)]) -> bool {
    params.iter().any(|(_, type_expr, default, _)| {
        type_expr.as_ref().is_some_and(type_refs_tidy)
            || default.as_ref().is_some_and(expr_refs_tidy)
    })
}

/// Returns whether a `use Trait` clause names a tidy class through its trait list or any
/// conflict-resolution adaptation.
fn trait_use_refs_tidy(trait_use: &TraitUse) -> bool {
    trait_use.trait_names.iter().any(name_is_tidy_class)
        || trait_use.adaptations.iter().any(|adaptation| match adaptation {
            TraitAdaptation::Alias { trait_name, .. } => {
                trait_name.as_ref().is_some_and(name_is_tidy_class)
            }
            TraitAdaptation::InsteadOf {
                trait_name,
                instead_of,
                ..
            } => {
                trait_name.as_ref().is_some_and(name_is_tidy_class)
                    || instead_of.iter().any(name_is_tidy_class)
            }
        })
}

/// Returns whether a class property's type hint or default value references the tidy
/// surface.
fn class_property_refs_tidy(property: &ClassProperty) -> bool {
    property.type_expr.as_ref().is_some_and(type_refs_tidy)
        || property.default.as_ref().is_some_and(expr_refs_tidy)
}

/// Returns whether a method's parameters, variadic element type, return type, or body
/// reference the tidy surface.
fn class_method_refs_tidy(method: &ClassMethod) -> bool {
    params_ref_tidy(&method.params)
        || method.variadic_type.as_ref().is_some_and(type_refs_tidy)
        || method.return_type.as_ref().is_some_and(type_refs_tidy)
        || method.body.iter().any(stmt_refs_tidy)
}

/// Returns whether a class constant's initializer references the tidy surface.
fn class_const_refs_tidy(constant: &ClassConst) -> bool {
    expr_refs_tidy(&constant.value)
}

/// Returns whether an enum case's backing-value expression references the tidy surface.
fn enum_case_refs_tidy(case: &EnumCaseDecl) -> bool {
    case.value.as_ref().is_some_and(expr_refs_tidy)
}

/// Returns whether a `packed class` field's type references a tidy class. A tidy class is
/// never a valid packed field type, but the field is walked for completeness.
fn packed_field_refs_tidy(field: &PackedField) -> bool {
    type_refs_tidy(&field.type_expr)
}

/// Returns whether an expression references the tidy surface at any function-call,
/// class-name, or constant position, recursing into every child expression and statement.
/// The `match` is exhaustive so a newly added `ExprKind` cannot silently bypass detection.
fn expr_refs_tidy(expr: &Expr) -> bool {
    match &expr.kind {
        // A literal naming a tidy function can become a runtime callable
        // (`$f = 'tidy_parse_string'; $f()`). Bare class-name literals are deliberately
        // NOT triggers: `'tidy'` is ordinary English data and `'tidyNode'` is too close to
        // one, while treating either as the class forced unrelated programs to depend on
        // the managed libtidy package. A dynamic `new $class` naming either class uses
        // `--with-tidy`.
        ExprKind::StringLiteral(value) => tidy_functions()
            .iter()
            .any(|name| value.eq_ignore_ascii_case(name)),
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
        | ExprKind::MagicConstant(_) => false,

        // `TIDY_*` constants resolve from the shared constant catalog with or without the
        // prelude, so a bare constant is not a trigger; the call or class-name position
        // that needs the prelude is caught elsewhere.
        ExprKind::ConstRef(_) => false,

        ExprKind::BinaryOp { left, right, .. } => expr_refs_tidy(left) || expr_refs_tidy(right),
        ExprKind::InstanceOf { value, target } => {
            expr_refs_tidy(value) || instanceof_target_refs_tidy(target)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::YieldFrom(inner) => expr_refs_tidy(inner),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_refs_tidy(value) || expr_refs_tidy(default)
        }
        ExprKind::Pipe { value, callable } => expr_refs_tidy(value) || expr_refs_tidy(callable),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            expr_refs_tidy(target)
                || expr_refs_tidy(value)
                || result_target.as_deref().is_some_and(expr_refs_tidy)
                || prelude.iter().any(stmt_refs_tidy)
        }
        // A free-function call is the dominant tidy position: `tidy_parse_string()` and
        // `tidy_parse_string()` mint the objects, so a program may parse or write
        // without ever naming a class.
        ExprKind::FunctionCall { name, args } => {
            name_is_tidy_function(name)
                || is_eval_call(name) && args.first().is_some_and(eval_source_refs_tidy)
                || args.iter().any(expr_refs_tidy)
        }
        ExprKind::ClosureCall { args, .. } => args.iter().any(expr_refs_tidy),
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_refs_tidy),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs
            .iter()
            .any(|(key, value)| expr_refs_tidy(key) || expr_refs_tidy(value)),
        ExprKind::ArrayLiteralMixed(entries) => entries
            .iter()
            .flat_map(|entry| entry.exprs())
            .any(expr_refs_tidy),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_refs_tidy(subject)
                || arms.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_tidy) || expr_refs_tidy(body)
                })
                || default.as_deref().is_some_and(expr_refs_tidy)
        }
        ExprKind::ArrayAccess { array, index } => expr_refs_tidy(array) || expr_refs_tidy(index),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => expr_refs_tidy(condition) || expr_refs_tidy(then_expr) || expr_refs_tidy(else_expr),
        ExprKind::Cast { expr, .. } | ExprKind::PtrCast { expr, .. } => expr_refs_tidy(expr),
        ExprKind::Closure {
            params,
            variadic_type,
            return_type,
            body,
            ..
        } => {
            params_ref_tidy(params)
                || variadic_type.as_ref().is_some_and(type_refs_tidy)
                || return_type.as_ref().is_some_and(type_refs_tidy)
                || body.iter().any(stmt_refs_tidy)
        }
        ExprKind::NamedArg { value, .. } => expr_refs_tidy(value),
        ExprKind::ExprCall { callee, args } => {
            expr_refs_tidy(callee) || args.iter().any(expr_refs_tidy)
        }
        ExprKind::NewObject { class_name, args } => {
            name_is_tidy_class(class_name) || args.iter().any(expr_refs_tidy)
        }
        ExprKind::NewGeneric { class_type, args } => {
            type_refs_tidy(class_type) || args.iter().any(expr_refs_tidy)
        }
        ExprKind::NewDynamic { name_expr, args } => {
            expr_refs_tidy(name_expr) || args.iter().any(expr_refs_tidy)
        }
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class,
            required_parent,
            args,
        } => {
            expr_refs_tidy(class_name)
                || name_is_tidy_class(fallback_class)
                || name_is_tidy_class(required_parent)
                || args.iter().any(expr_refs_tidy)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_refs_tidy(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_refs_tidy(object) || expr_refs_tidy(property)
        }
        ExprKind::StaticPropertyAccess { receiver, .. } => receiver_refs_tidy(receiver),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_refs_tidy(object) || args.iter().any(expr_refs_tidy)
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => expr_refs_tidy(object) || expr_refs_tidy(method) || args.iter().any(expr_refs_tidy),
        ExprKind::StaticMethodCall { receiver, args, .. } => {
            receiver_refs_tidy(receiver) || args.iter().any(expr_refs_tidy)
        }
        ExprKind::FirstClassCallable(target) => callable_target_refs_tidy(target),
        ExprKind::BufferNew { element_type, len } => {
            type_refs_tidy(element_type) || expr_refs_tidy(len)
        }
        ExprKind::ClassConstant { receiver }
        | ExprKind::ScopedConstantAccess { receiver, .. } => receiver_refs_tidy(receiver),
        ExprKind::ObjectClassName { object } => expr_refs_tidy(object),
        ExprKind::NewScopedObject { receiver, args } => {
            receiver_refs_tidy(receiver) || args.iter().any(expr_refs_tidy)
        }
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr_refs_tidy)
                || value.as_deref().is_some_and(expr_refs_tidy)
        }
        // Transient: the resolver expands this into the included file's statements before
        // tidy detection runs, so it should never reach here. Recurse into the path
        // expression defensively to keep detection exhaustive and correct.
        ExprKind::IncludeValue { path, .. } => expr_refs_tidy(path),
    }
}

/// Returns whether a statement references the tidy surface at any function-call,
/// class-name, or constant position, recursing into nested statements, expressions, and
/// class members. The `match` is exhaustive so a newly added `StmtKind` cannot silently
/// bypass detection.
fn stmt_refs_tidy(stmt: &Stmt) -> bool {
    match &stmt.kind {
        // Statements with no tidy-name position and no child expr/stmt.
        StmtKind::RefAssign { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::Global { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => false,

        // An aliased import (`use tidyNode as W;`) names the class only here; the later
        // `new W()` / `W $w` carries the alias, which the walk cannot otherwise connect
        // back — so skipping imports would be a false negative. Function imports
        // (`use function tidy_parse as p;`) land in the same list and are caught by the
        // same check, which is why both name tests are applied.
        StmtKind::UseDecl { imports } => imports
            .iter()
            .any(|item| name_is_tidy_class(&item.name) || name_is_tidy_function(&item.name)),

        StmtKind::Echo(expr) | StmtKind::Throw(expr) | StmtKind::ExprStmt(expr) => {
            expr_refs_tidy(expr)
        }
        StmtKind::Assign { value, .. } => expr_refs_tidy(value),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_refs_tidy(condition)
                || then_body.iter().any(stmt_refs_tidy)
                || elseif_clauses
                    .iter()
                    .any(|(cond, body)| expr_refs_tidy(cond) || body.iter().any(stmt_refs_tidy))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_tidy))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_refs_tidy)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_tidy))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_refs_tidy(condition) || body.iter().any(stmt_refs_tidy)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_refs_tidy)
                || condition.as_ref().is_some_and(expr_refs_tidy)
                || update.as_deref().is_some_and(stmt_refs_tidy)
                || body.iter().any(stmt_refs_tidy)
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            expr_refs_tidy(index) || expr_refs_tidy(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_refs_tidy(target) || expr_refs_tidy(value)
        }
        StmtKind::ArrayPush { value, .. } => expr_refs_tidy(value),
        StmtKind::TypedAssign {
            type_expr, value, ..
        } => type_refs_tidy(type_expr) || expr_refs_tidy(value),
        StmtKind::Foreach { array, body, .. } => {
            expr_refs_tidy(array) || body.iter().any(stmt_refs_tidy)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_refs_tidy(subject)
                || cases.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_tidy) || body.iter().any(stmt_refs_tidy)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_tidy))
        }
        StmtKind::Include { path, .. } => expr_refs_tidy(path),
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => body.iter().any(stmt_refs_tidy),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_refs_tidy)
                || catches.iter().any(|catch| {
                    catch.exception_types.iter().any(name_is_tidy_class)
                        || catch.body.iter().any(stmt_refs_tidy)
                })
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_tidy))
        }
        StmtKind::FunctionDecl {
            params,
            variadic_type,
            return_type,
            body,
            ..
        } => {
            params_ref_tidy(params)
                || variadic_type.as_ref().is_some_and(type_refs_tidy)
                || return_type.as_ref().is_some_and(type_refs_tidy)
                || body.iter().any(stmt_refs_tidy)
        }
        StmtKind::Return(value) => value.as_ref().is_some_and(expr_refs_tidy),
        StmtKind::ConstDecl { value, .. } => expr_refs_tidy(value),
        StmtKind::ListUnpack { value, .. } => expr_refs_tidy(value),
        StmtKind::StaticVar { init, .. } => expr_refs_tidy(init),
        StmtKind::ClassDecl {
            extends,
            implements,
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.as_ref().is_some_and(name_is_tidy_class)
                || implements.iter().any(name_is_tidy_class)
                || trait_uses.iter().any(trait_use_refs_tidy)
                || properties.iter().any(class_property_refs_tidy)
                || methods.iter().any(class_method_refs_tidy)
                || constants.iter().any(class_const_refs_tidy)
        }
        // An enum carries the same member kinds as a class (methods, constants, trait
        // uses, implemented interfaces) on top of its backing type and cases; a program
        // whose only tidy call sits in an enum method must still inject the prelude.
        StmtKind::EnumDecl {
            backing_type,
            cases,
            implements,
            trait_uses,
            methods,
            constants,
            ..
        } => {
            backing_type.as_ref().is_some_and(type_refs_tidy)
                || cases.iter().any(enum_case_refs_tidy)
                || implements.iter().any(name_is_tidy_class)
                || trait_uses.iter().any(trait_use_refs_tidy)
                || methods.iter().any(class_method_refs_tidy)
                || constants.iter().any(class_const_refs_tidy)
        }
        StmtKind::PackedClassDecl { fields, .. } => fields.iter().any(packed_field_refs_tidy),
        StmtKind::InterfaceDecl {
            extends,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.iter().any(name_is_tidy_class)
                || properties.iter().any(class_property_refs_tidy)
                || methods.iter().any(class_method_refs_tidy)
                || constants.iter().any(class_const_refs_tidy)
        }
        StmtKind::TraitDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            trait_uses.iter().any(trait_use_refs_tidy)
                || properties.iter().any(class_property_refs_tidy)
                || methods.iter().any(class_method_refs_tidy)
                || constants.iter().any(class_const_refs_tidy)
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_refs_tidy(object) || expr_refs_tidy(value)
        }
        StmtKind::StaticPropertyAssign {
            receiver, value, ..
        }
        | StmtKind::StaticPropertyArrayPush {
            receiver, value, ..
        } => receiver_refs_tidy(receiver) || expr_refs_tidy(value),
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            index,
            value,
            ..
        } => receiver_refs_tidy(receiver) || expr_refs_tidy(index) || expr_refs_tidy(value),
        StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_refs_tidy(object) || expr_refs_tidy(value)
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => expr_refs_tidy(object) || expr_refs_tidy(index) || expr_refs_tidy(value),
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the tidy AST walk: `tidy_*` / `ob_tidyhandler` calls and
    //! `tidy` / `tidyNode` class-name positions are detected across qualified and
    //! mixed-case spellings, while unrelated helper names and constants are not.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests parse raw source (pre name-resolution), matching the stage at which
    //!   `program_uses_tidy` runs inside `inject_if_used`.

    use super::*;

    /// Parses source the same way `inject_if_used` sees it: tokenize then parse, before
    /// any name resolution.
    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// The catalog names the 24 functions plus `ob_tidyhandler`.
    #[test]
    fn the_function_set_is_the_catalog() {
        assert_eq!(tidy_functions().len(), 25);
        assert!(tidy_functions().contains(&"tidy_parse_string"));
        assert!(tidy_functions().contains(&"tidy_get_output"));
        assert!(tidy_functions().contains(&"ob_tidyhandler"));
        assert_eq!(tidy_classes(), &["tidy", "tidyNode"]);
    }

    /// Every enumerated function is detected on its own.
    #[test]
    fn detects_every_enumerated_tidy_function() {
        for name in tidy_functions() {
            let source = format!("<?php function f($a, $b, $c) {{ return {name}($a, $b, $c); }}");
            assert!(
                program_uses_tidy(&parse(&source)),
                "{name} must trigger tidy prelude injection"
            );
        }
    }

    /// Both classes are detected in a parameter type hint.
    #[test]
    fn detects_every_class_type_hint() {
        for class in tidy_classes() {
            let source = format!("<?php function f({class} $c): bool {{ return true; }}");
            assert!(
                program_uses_tidy(&parse(&source)),
                "{class} must trigger tidy prelude injection"
            );
        }
    }

    /// `new tidy()`, static receivers and `instanceof` are class-name positions.
    #[test]
    fn detects_class_name_positions() {
        assert!(program_uses_tidy(&parse("<?php $t = new tidy();")));
        assert!(program_uses_tidy(&parse("<?php $t = new tidyNode(1);")));
        assert!(program_uses_tidy(&parse("<?php $t = tidy::repairString('<p>x</p>');")));
        assert!(program_uses_tidy(&parse("<?php if ($x instanceof tidyNode) { echo 1; }")));
        assert!(program_uses_tidy(&parse("<?php class W extends tidy {}")));
    }

    /// Fully-qualified global functions/classes remain case-insensitive.
    #[test]
    fn detects_fully_qualified_and_case_insensitive() {
        assert!(program_uses_tidy(&parse(r#"<?php $p = \TIDY_PARSE_STRING('<p>x</p>');"#)));
        assert!(program_uses_tidy(&parse(r#"<?php function f(\tidynode $w): bool { return true; }"#)));
    }

    /// An aliased import is detected through the import name.
    #[test]
    fn detects_aliased_use_import() {
        assert!(program_uses_tidy(&parse("<?php use tidyNode as N; function f(N $n): bool { return true; }")));
        assert!(program_uses_tidy(&parse("<?php use function tidy_parse_string as p; $r = p($x);")));
    }

    /// An enum method body is a call position like a class method's.
    #[test]
    fn detects_enum_methods() {
        assert!(program_uses_tidy(&parse(
            "<?php enum Fmt { case Tidy; public function parse(string $d): bool { $t = tidy_parse_string($d); return tidy_get_status($t) === 0; } }"
        )));
        assert!(program_uses_tidy(&parse(
            "<?php enum Fmt { case Tidy; public static function make(): tidyNode { return tidyNode::__elephc_wrap(1); } }"
        )));
    }

    /// A typed variadic parameter is a class-name position.
    #[test]
    fn detects_variadic_parameter_types() {
        assert!(program_uses_tidy(&parse("<?php function f(tidyNode ...$nodes): int { return count($nodes); }")));
        assert!(program_uses_tidy(&parse("<?php class C { public function m(tidy ...$docs): int { return count($docs); } }")));
        assert!(!program_uses_tidy(&parse("<?php function f(int ...$xs): int { return count($xs); }")));
    }

    /// An unqualified call inside a namespace falls back to the global function.
    #[test]
    fn detects_namespaced_function_fallback() {
        assert!(program_uses_tidy(&parse("<?php namespace App; $t = tidy_parse_string('<p>x</p>');")));
    }

    /// The `ob_tidyhandler` callback name is detected either as a call or a string, and a
    /// tidy function name used as a runtime callable is detected without treating ordinary
    /// data such as the word `tidy` as a class reference.
    #[test]
    fn detects_callable_strings_but_not_bare_class_words() {
        assert!(program_uses_tidy(&parse("<?php ob_start('ob_tidyhandler');")));
        assert!(program_uses_tidy(&parse(r#"<?php ob_start("OB_TIDYHANDLER");"#)));
        assert!(program_uses_tidy(&parse("<?php $f = 'tidy_parse_string'; $f('<p>x</p>');")));
        assert!(!program_uses_tidy(&parse("<?php echo 'tidy';")));
        assert!(!program_uses_tidy(&parse("<?php echo 'tidyNode';")));
        assert!(program_uses_tidy(&parse("<?php $class = tidyNode::class; new $class;")));
    }

    /// A literal `eval()` source mentioning the surface pulls in the prelude too: the
    /// compiler does not parse runtime source, but it can see the names it will look up later.
    #[test]
    fn detects_literal_eval_fragments() {
        assert!(program_uses_tidy(&parse(
            r#"<?php eval('$t = tidy_parse_string("<p>x</p>");');"#
        )));
        assert!(program_uses_tidy(&parse(
            r#"<?php eval('ob_start("ob_tidyhandler");');"#
        )));
        assert!(program_uses_tidy(&parse(
            r#"<?php eval('$t = new tidy();');"#
        )));
        assert!(!program_uses_tidy(&parse(
            r#"<?php eval('$value = tidy_helper(1);');"#
        )));
        assert!(!program_uses_tidy(&parse(
            r#"<?php eval('$tidy = 1;');"#
        )));
        assert!(!program_uses_tidy(&parse(
            r#"<?php eval('$box->tidy();');"#
        )));
        assert!(!program_uses_tidy(&parse(
            r#"<?php eval('$mode = "tidy";');"#
        )));
    }

    /// Fully-qualified user symbols that merely end in tidy names stay in their namespace.
    #[test]
    fn ignores_fully_qualified_user_symbols() {
        assert!(!program_uses_tidy(&parse(r#"<?php $value = \Some\tidy_parse_string($handle);"#)));
        assert!(!program_uses_tidy(&parse(r#"<?php function f(\Acme\tidyNode $w): bool { return true; }"#)));
    }

    /// A user helper with a `tidy_` prefix and a `TIDY_*` constant alone do not inject.
    #[test]
    fn ignores_unrelated_programs() {
        assert!(!program_uses_tidy(&parse("<?php function tidy_helper($x) { return $x; } echo tidy_helper(1);")));
        assert!(!program_uses_tidy(&parse("<?php echo TIDY_NODETYPE_ROOT;")));
    }
}
