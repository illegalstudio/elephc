//! Purpose:
//! Decides whether a parsed program references PHP's `ext/curl` surface — any of the
//! `curl_*` functions, any of the `Curl*`/`CURL*` classes, or any `CURLOPT_*` /
//! `CURLINFO_*` / `CURLE_*` / `CURL_*` constant — so the curl prelude is injected only
//! for programs that actually use curl.
//!
//! Called from:
//! - `crate::curl_prelude::inject_if_used`.
//!
//! Key details:
//! - Runs before name resolution, so `Name`s are raw source text and PHP function and
//!   class names are case-insensitive. A reference may be written `curl_init`,
//!   `\curl_init`, or `\Some\curl_init`, and `CurlHandle` may be spelled
//!   `\CurlHandle` or `curlhandle`. The walk therefore matches the unqualified last
//!   segment case-insensitively, exactly like `hash_prelude::detect`, which this file
//!   mirrors structurally.
//! - THREE KINDS OF POSITION trigger injection: (a) a *function call* naming one of
//!   the `curl_*` functions — the dominant way the surface is used, since `curl_init()`
//!   is what mints a `CurlHandle` in the first place; (b) a *class-name* position
//!   naming one of curl's six classes (new, static receivers, `instanceof`, `catch`,
//!   `extends`/`implements`, type hints, trait uses, `use` imports); and (c) a
//!   *constant* reference whose name carries one of curl's four frozen prefixes, so a
//!   program that only mentions `CURLOPT_URL` still injects.
//! - THE FUNCTION SET IS WHOLE-SEGMENT, NOT A BARE `curl` PREFIX. The exact names are
//!   listed, plus the two families (`curl_multi_*`, `curl_share_*`) matched by their
//!   `curl_multi_`/`curl_share_` prefix rather than individually enumerated. A bare
//!   `curl` prefix test would inject (and bridge-link) for a user's own
//!   `curl_helper()`; requiring the `curl_multi_`/`curl_share_` underscore keeps the
//!   family match as tight as an enumeration while staying forward-compatible with any
//!   future addition to either family.
//! - Soundness over precision: a missed reference would drop the prelude and turn a
//!   valid program into an "undefined function/class" error, so the `match`es are
//!   exhaustive (no wildcard arm). Adding an AST node forces this file to be updated.
//!   A false positive costs more here than it does for the hash prelude — the injected
//!   declarations call the internal `__elephc_curl_*` builtins, which declare
//!   `Bridge("elephc_curl")` and therefore require the managed native `curl` package —
//!   which is exactly the pay-for-use guarantee this crate holds for every optional
//!   native surface, so the
//!   matching stays deliberately narrow rather than "anything starting with curl".
//! - CONSTANT PREFIXES ARE MATCHED CASE-INSENSITIVELY for consistency with the rest of
//!   this file, even though PHP constants are case-sensitive. The four frozen prefixes
//!   are `CURLOPT_`/`CURLINFO_`/`CURLE_`/`CURL_`; other real families (`CURLM_*`, `CURLPROTO_*`,
//!   `CURLAUTH_*`) are deliberately NOT matched, because a constant on its own does no
//!   curl work: it resolves through the builtin constant table unconditionally, with or
//!   without
//!   the prelude, and the call or class-name position that actually needs the prelude is
//!   caught by (a) or (b) above.

use crate::names::Name;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    InstanceOfTarget, PackedField, StaticReceiver, Stmt, StmtKind, TraitAdaptation, TraitUse,
    TypeExpr,
};

/// The PHP functions the curl prelude declares or reserves. Whole-segment matches: every
/// name here is a real `ext/curl` function, and nothing else pulls the prelude in.
const CURL_FUNCTIONS: &[&str] = &[
    "curl_init",
    "curl_setopt",
    "curl_setopt_array",
    "curl_exec",
    "curl_close",
    "curl_copy_handle",
    "curl_errno",
    "curl_error",
    "curl_escape",
    "curl_unescape",
    "curl_getinfo",
    "curl_pause",
    "curl_reset",
    "curl_upkeep",
    "curl_version",
    "curl_strerror",
    "curl_file_create",
];

/// The `curl_multi_*` / `curl_share_*` families, matched by prefix because their member
/// lists are still growing (Tasks 9 and 10). The trailing underscore is part of the
/// prefix, so a user's `curl_multiplexer()` does not match.
const CURL_FUNCTION_FAMILIES: &[&str] = &["curl_multi_", "curl_share_"];

/// The six OOP classes of PHP's curl surface. `CurlSharePersistentHandle` is 8.5-only and
/// `CURLStringFile` is 8.1+, but detection is version-independent: naming any of them is
/// enough to want the prelude, and the prelude itself decides what to declare.
const CURL_CLASSES: &[&str] = &[
    "CurlHandle",
    "CurlMultiHandle",
    "CurlShareHandle",
    "CurlSharePersistentHandle",
    "CURLFile",
    "CURLStringFile",
];

/// The four frozen constant prefixes from the task brief.
const CURL_CONSTANT_PREFIXES: &[&str] = &["CURLOPT_", "CURLINFO_", "CURLE_", "CURL_"];

/// Returns whether any top-level statement references the curl surface, so the prelude
/// must be injected ahead of user code.
pub(super) fn program_uses_curl(program: &[Stmt]) -> bool {
    program.iter().any(stmt_refs_curl)
}

/// Returns whether `name`'s unqualified last segment is a curl function, compared
/// case-insensitively to match PHP's case-insensitive function names and any
/// namespace/leading-backslash form (`curl_init`, `\curl_init`, `\Some\curl_init`).
fn name_is_curl_function(name: &Name) -> bool {
    name.last_segment().is_some_and(|segment| {
        CURL_FUNCTIONS
            .iter()
            .any(|candidate| segment.eq_ignore_ascii_case(candidate))
            || CURL_FUNCTION_FAMILIES.iter().any(|family| {
                segment.len() > family.len()
                    && segment.as_bytes()[..family.len()].eq_ignore_ascii_case(family.as_bytes())
            })
    })
}

/// Returns whether `name`'s unqualified last segment is one of curl's classes, compared
/// case-insensitively and tolerant of any namespace/leading-backslash form.
fn name_is_curl_class(name: &Name) -> bool {
    name.last_segment().is_some_and(|segment| {
        CURL_CLASSES
            .iter()
            .any(|candidate| segment.eq_ignore_ascii_case(candidate))
    })
}

/// Returns whether `name`'s unqualified last segment carries one of curl's constant
/// prefixes (`CURLOPT_URL`, `CURLINFO_HTTP_CODE`, `CURLE_OK`, `CURL_HTTP_VERSION_2_0`).
fn name_is_curl_constant(name: &Name) -> bool {
    name.last_segment().is_some_and(|segment| {
        CURL_CONSTANT_PREFIXES.iter().any(|prefix| {
            segment.len() > prefix.len()
                && segment.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
        })
    })
}

/// Returns whether a static receiver names a curl class (`CURLFile::...`).
/// `self`, `static`, and `parent` never resolve to a curl class at this position.
fn receiver_refs_curl(receiver: &StaticReceiver) -> bool {
    matches!(receiver, StaticReceiver::Named(name) if name_is_curl_class(name))
}

/// Returns whether an `instanceof` target references a curl class, recursing into the
/// operand when the target is a runtime expression.
fn instanceof_target_refs_curl(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(name) => name_is_curl_class(name),
        InstanceOfTarget::Expr(expr) => expr_refs_curl(expr),
    }
}

/// Returns whether a first-class-callable target references the curl surface: a free
/// function (`curl_exec(...)`), a static-method receiver, or an instance-method object
/// expression.
fn callable_target_refs_curl(target: &CallableTarget) -> bool {
    match target {
        CallableTarget::Function(name) => name_is_curl_function(name),
        CallableTarget::StaticMethod { receiver, .. } => receiver_refs_curl(receiver),
        CallableTarget::Method { object, .. } => expr_refs_curl(object),
    }
}

/// Returns whether a type expression names a curl class, recursing through
/// nullable/union/array/buffer wrappers and `ptr<Class>` targets.
fn type_refs_curl(type_expr: &TypeExpr) -> bool {
    match type_expr {
        TypeExpr::Int
        | TypeExpr::Float
        | TypeExpr::Bool
        | TypeExpr::False
        | TypeExpr::Str
        | TypeExpr::Void
        | TypeExpr::Never
        | TypeExpr::Iterable => false,
        TypeExpr::Ptr(target) => target.as_ref().is_some_and(name_is_curl_class),
        TypeExpr::Array(inner) | TypeExpr::Buffer(inner) | TypeExpr::Nullable(inner) => {
            type_refs_curl(inner)
        }
        TypeExpr::Named(name) => name_is_curl_class(name),
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            members.iter().any(type_refs_curl)
        }
    }
}

/// Returns whether any parameter's type hint or default value references the curl
/// surface. Shared by function, method, and closure parameter lists.
fn params_ref_curl(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)]) -> bool {
    params.iter().any(|(_, type_expr, default, _)| {
        type_expr.as_ref().is_some_and(type_refs_curl)
            || default.as_ref().is_some_and(expr_refs_curl)
    })
}

/// Returns whether a `use Trait` clause names a curl class through its trait list or any
/// conflict-resolution adaptation.
fn trait_use_refs_curl(trait_use: &TraitUse) -> bool {
    trait_use.trait_names.iter().any(name_is_curl_class)
        || trait_use.adaptations.iter().any(|adaptation| match adaptation {
            TraitAdaptation::Alias { trait_name, .. } => {
                trait_name.as_ref().is_some_and(name_is_curl_class)
            }
            TraitAdaptation::InsteadOf {
                trait_name,
                instead_of,
                ..
            } => {
                trait_name.as_ref().is_some_and(name_is_curl_class)
                    || instead_of.iter().any(name_is_curl_class)
            }
        })
}

/// Returns whether a class property's type hint or default value references the curl
/// surface.
fn class_property_refs_curl(property: &ClassProperty) -> bool {
    property.type_expr.as_ref().is_some_and(type_refs_curl)
        || property.default.as_ref().is_some_and(expr_refs_curl)
}

/// Returns whether a method's parameters, return type, or body reference the curl
/// surface.
fn class_method_refs_curl(method: &ClassMethod) -> bool {
    params_ref_curl(&method.params)
        || method.return_type.as_ref().is_some_and(type_refs_curl)
        || method.body.iter().any(stmt_refs_curl)
}

/// Returns whether a class constant's initializer references the curl surface.
fn class_const_refs_curl(constant: &ClassConst) -> bool {
    expr_refs_curl(&constant.value)
}

/// Returns whether an enum case's backing-value expression references the curl surface.
fn enum_case_refs_curl(case: &EnumCaseDecl) -> bool {
    case.value.as_ref().is_some_and(expr_refs_curl)
}

/// Returns whether a `packed class` field's type references a curl class. A curl class is
/// never a valid packed field type, but the field is walked for completeness.
fn packed_field_refs_curl(field: &PackedField) -> bool {
    type_refs_curl(&field.type_expr)
}

/// Returns whether an expression references the curl surface at any function-call,
/// class-name, or constant position, recursing into every child expression and statement.
/// The `match` is exhaustive so a newly added `ExprKind` cannot silently bypass detection.
fn expr_refs_curl(expr: &Expr) -> bool {
    match &expr.kind {
        // Leaves and identifier-only forms carry no curl reference.
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
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

        // A bare constant is the third trigger position: `curl_setopt($ch, CURLOPT_URL, …)`
        // names the constant, and a program that mentions only `CURLOPT_URL` still asks for
        // the curl surface.
        ExprKind::ConstRef(name) => name_is_curl_constant(name),

        ExprKind::BinaryOp { left, right, .. } => expr_refs_curl(left) || expr_refs_curl(right),
        ExprKind::InstanceOf { value, target } => {
            expr_refs_curl(value) || instanceof_target_refs_curl(target)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::YieldFrom(inner) => expr_refs_curl(inner),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_refs_curl(value) || expr_refs_curl(default)
        }
        ExprKind::Pipe { value, callable } => expr_refs_curl(value) || expr_refs_curl(callable),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            expr_refs_curl(target)
                || expr_refs_curl(value)
                || result_target.as_deref().is_some_and(expr_refs_curl)
                || prelude.iter().any(stmt_refs_curl)
        }
        // A free-function call is the dominant curl position: `curl_init()` is the only
        // way to obtain a `CurlHandle` in the first place, so a program may drive a
        // transfer without ever naming the class.
        ExprKind::FunctionCall { name, args } => {
            name_is_curl_function(name) || args.iter().any(expr_refs_curl)
        }
        ExprKind::ClosureCall { args, .. } => args.iter().any(expr_refs_curl),
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_refs_curl),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs
            .iter()
            .any(|(key, value)| expr_refs_curl(key) || expr_refs_curl(value)),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_refs_curl(subject)
                || arms.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_curl) || expr_refs_curl(body)
                })
                || default.as_deref().is_some_and(expr_refs_curl)
        }
        ExprKind::ArrayAccess { array, index } => expr_refs_curl(array) || expr_refs_curl(index),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => expr_refs_curl(condition) || expr_refs_curl(then_expr) || expr_refs_curl(else_expr),
        ExprKind::Cast { expr, .. } | ExprKind::PtrCast { expr, .. } => expr_refs_curl(expr),
        ExprKind::Closure {
            params,
            return_type,
            body,
            ..
        } => {
            params_ref_curl(params)
                || return_type.as_ref().is_some_and(type_refs_curl)
                || body.iter().any(stmt_refs_curl)
        }
        ExprKind::NamedArg { value, .. } => expr_refs_curl(value),
        ExprKind::ExprCall { callee, args } => {
            expr_refs_curl(callee) || args.iter().any(expr_refs_curl)
        }
        ExprKind::NewObject { class_name, args } => {
            name_is_curl_class(class_name) || args.iter().any(expr_refs_curl)
        }
        ExprKind::NewDynamic { name_expr, args } => {
            expr_refs_curl(name_expr) || args.iter().any(expr_refs_curl)
        }
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class,
            required_parent,
            args,
        } => {
            expr_refs_curl(class_name)
                || name_is_curl_class(fallback_class)
                || name_is_curl_class(required_parent)
                || args.iter().any(expr_refs_curl)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_refs_curl(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_refs_curl(object) || expr_refs_curl(property)
        }
        ExprKind::StaticPropertyAccess { receiver, .. } => receiver_refs_curl(receiver),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_refs_curl(object) || args.iter().any(expr_refs_curl)
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => expr_refs_curl(object) || expr_refs_curl(method) || args.iter().any(expr_refs_curl),
        ExprKind::StaticMethodCall { receiver, args, .. } => {
            receiver_refs_curl(receiver) || args.iter().any(expr_refs_curl)
        }
        ExprKind::FirstClassCallable(target) => callable_target_refs_curl(target),
        ExprKind::BufferNew { element_type, len } => {
            type_refs_curl(element_type) || expr_refs_curl(len)
        }
        ExprKind::ClassConstant { receiver }
        | ExprKind::ScopedConstantAccess { receiver, .. } => receiver_refs_curl(receiver),
        ExprKind::ObjectClassName { object } => expr_refs_curl(object),
        ExprKind::NewScopedObject { receiver, args } => {
            receiver_refs_curl(receiver) || args.iter().any(expr_refs_curl)
        }
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr_refs_curl)
                || value.as_deref().is_some_and(expr_refs_curl)
        }
        // Transient: the resolver expands this into the included file's statements before
        // curl detection runs, so it should never reach here. Recurse into the path
        // expression defensively to keep detection exhaustive and correct.
        ExprKind::IncludeValue { path, .. } => expr_refs_curl(path),
    }
}

/// Returns whether a statement references the curl surface at any function-call,
/// class-name, or constant position, recursing into nested statements, expressions, and
/// class members. The `match` is exhaustive so a newly added `StmtKind` cannot silently
/// bypass detection.
fn stmt_refs_curl(stmt: &Stmt) -> bool {
    match &stmt.kind {
        // Statements with no curl-name position and no child expr/stmt.
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

        // An aliased import (`use CurlHandle as Ch;`) names the class only here; the later
        // `new Ch()` / `Ch $c` carries the alias, which the walk cannot otherwise connect
        // back — so skipping imports would be a false negative. Function imports
        // (`use function curl_init as ci;`) land in the same list and are caught by the
        // same check, which is why both name tests are applied.
        StmtKind::UseDecl { imports } => imports
            .iter()
            .any(|item| name_is_curl_class(&item.name) || name_is_curl_function(&item.name)),

        StmtKind::Echo(expr) | StmtKind::Throw(expr) | StmtKind::ExprStmt(expr) => {
            expr_refs_curl(expr)
        }
        StmtKind::Assign { value, .. } => expr_refs_curl(value),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_refs_curl(condition)
                || then_body.iter().any(stmt_refs_curl)
                || elseif_clauses
                    .iter()
                    .any(|(cond, body)| expr_refs_curl(cond) || body.iter().any(stmt_refs_curl))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_curl))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_refs_curl)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_curl))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_refs_curl(condition) || body.iter().any(stmt_refs_curl)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_refs_curl)
                || condition.as_ref().is_some_and(expr_refs_curl)
                || update.as_deref().is_some_and(stmt_refs_curl)
                || body.iter().any(stmt_refs_curl)
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            expr_refs_curl(index) || expr_refs_curl(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_refs_curl(target) || expr_refs_curl(value)
        }
        StmtKind::ArrayPush { value, .. } => expr_refs_curl(value),
        StmtKind::TypedAssign {
            type_expr, value, ..
        } => type_refs_curl(type_expr) || expr_refs_curl(value),
        StmtKind::Foreach { array, body, .. } => {
            expr_refs_curl(array) || body.iter().any(stmt_refs_curl)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_refs_curl(subject)
                || cases.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_curl) || body.iter().any(stmt_refs_curl)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_curl))
        }
        StmtKind::Include { path, .. } => expr_refs_curl(path),
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => body.iter().any(stmt_refs_curl),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_refs_curl)
                || catches.iter().any(|catch| {
                    catch.exception_types.iter().any(name_is_curl_class)
                        || catch.body.iter().any(stmt_refs_curl)
                })
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_curl))
        }
        StmtKind::FunctionDecl {
            params,
            return_type,
            body,
            ..
        } => {
            params_ref_curl(params)
                || return_type.as_ref().is_some_and(type_refs_curl)
                || body.iter().any(stmt_refs_curl)
        }
        StmtKind::Return(value) => value.as_ref().is_some_and(expr_refs_curl),
        StmtKind::ConstDecl { value, .. } => expr_refs_curl(value),
        StmtKind::ListUnpack { value, .. } => expr_refs_curl(value),
        StmtKind::StaticVar { init, .. } => expr_refs_curl(init),
        StmtKind::ClassDecl {
            extends,
            implements,
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.as_ref().is_some_and(name_is_curl_class)
                || implements.iter().any(name_is_curl_class)
                || trait_uses.iter().any(trait_use_refs_curl)
                || properties.iter().any(class_property_refs_curl)
                || methods.iter().any(class_method_refs_curl)
                || constants.iter().any(class_const_refs_curl)
        }
        StmtKind::EnumDecl {
            backing_type,
            cases,
            ..
        } => {
            backing_type.as_ref().is_some_and(type_refs_curl)
                || cases.iter().any(enum_case_refs_curl)
        }
        StmtKind::PackedClassDecl { fields, .. } => fields.iter().any(packed_field_refs_curl),
        StmtKind::InterfaceDecl {
            extends,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.iter().any(name_is_curl_class)
                || properties.iter().any(class_property_refs_curl)
                || methods.iter().any(class_method_refs_curl)
                || constants.iter().any(class_const_refs_curl)
        }
        StmtKind::TraitDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            trait_uses.iter().any(trait_use_refs_curl)
                || properties.iter().any(class_property_refs_curl)
                || methods.iter().any(class_method_refs_curl)
                || constants.iter().any(class_const_refs_curl)
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_refs_curl(object) || expr_refs_curl(value)
        }
        StmtKind::StaticPropertyAssign {
            receiver, value, ..
        }
        | StmtKind::StaticPropertyArrayPush {
            receiver, value, ..
        } => receiver_refs_curl(receiver) || expr_refs_curl(value),
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            index,
            value,
            ..
        } => receiver_refs_curl(receiver) || expr_refs_curl(index) || expr_refs_curl(value),
        StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_refs_curl(object) || expr_refs_curl(value)
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => expr_refs_curl(object) || expr_refs_curl(index) || expr_refs_curl(value),
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the curl AST walk: every `curl_*` call, every `Curl*`/`CURL*`
    //! class-name position, and every frozen constant prefix is detected across
    //! `\`-qualified and mixed-case spellings, while unrelated `curl`-shaped user names
    //! and non-call mentions are not.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests parse raw source (pre name-resolution), matching the stage at which
    //!   `program_uses_curl` runs inside `inject_if_used`.

    use super::*;

    /// Parses source the same way `inject_if_used` sees it: tokenize then parse, before
    /// any name resolution.
    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// Every enumerated `curl_*` function is detected on its own, so a helper that only
    /// consumes a handle passed in still pulls the prelude.
    #[test]
    fn detects_every_enumerated_curl_function() {
        for name in CURL_FUNCTIONS {
            let source = format!("<?php function f($a, $b, $c) {{ return {name}($a, $b, $c); }}");
            assert!(
                program_uses_curl(&parse(&source)),
                "{name} must trigger curl prelude injection"
            );
        }
    }

    /// The `curl_multi_*` / `curl_share_*` families are detected by prefix, so Tasks 9
    /// and 10 can add members without editing this file.
    #[test]
    fn detects_multi_and_share_families() {
        assert!(program_uses_curl(&parse("<?php $m = curl_multi_init();")));
        assert!(program_uses_curl(&parse(
            "<?php function f($m, $h) { curl_multi_add_handle($m, $h); }"
        )));
        assert!(program_uses_curl(&parse("<?php $s = curl_share_init();")));
        assert!(program_uses_curl(&parse(
            "<?php function f($s) { return curl_share_errno($s); }"
        )));
    }

    /// Every curl class is detected in a parameter type hint, even when the body never
    /// calls a curl function.
    #[test]
    fn detects_every_class_type_hint() {
        for class in CURL_CLASSES {
            let source = format!("<?php function f({class} $c): bool {{ return true; }}");
            assert!(
                program_uses_curl(&parse(&source)),
                "{class} must trigger curl prelude injection"
            );
        }
    }

    /// An `instanceof CurlHandle` check is detected.
    #[test]
    fn detects_instanceof() {
        assert!(program_uses_curl(&parse(
            "<?php if ($x instanceof CurlHandle) { echo 1; }"
        )));
    }

    /// A `new CURLFile(...)` is detected at the class-name position.
    #[test]
    fn detects_new_object() {
        assert!(program_uses_curl(&parse(
            r#"<?php $f = new CURLFile("/tmp/a.txt");"#
        )));
    }

    /// A `catch (CurlHandle $e)`-shaped class-name position is detected.
    #[test]
    fn detects_catch_class_name() {
        assert!(program_uses_curl(&parse(
            "<?php try { echo 1; } catch (CurlHandle $e) { echo 2; }"
        )));
    }

    /// Each frozen constant prefix is detected on its own, so a program that only
    /// mentions `CURLOPT_URL` still injects the prelude.
    #[test]
    fn detects_constant_prefixes() {
        for constant in [
            "CURLOPT_URL",
            "CURLOPT_RETURNTRANSFER",
            "CURLINFO_HTTP_CODE",
            "CURLE_OK",
            "CURL_HTTP_VERSION_2_0",
        ] {
            let source = format!("<?php $o = {constant};");
            assert!(
                program_uses_curl(&parse(&source)),
                "{constant} must trigger curl prelude injection"
            );
        }
    }

    /// A constant used as a call argument — the way `CURLOPT_*` is actually written — is
    /// detected through the argument walk.
    #[test]
    fn detects_constant_inside_call_argument() {
        assert!(program_uses_curl(&parse(
            "<?php function f($ch) { return elephc_option($ch, CURLOPT_URL); }"
        )));
    }

    /// A fully-qualified, differently-cased reference is detected on the function side,
    /// the class side, and the constant side (last segment, case-insensitive).
    #[test]
    fn detects_fully_qualified_and_case_insensitive() {
        assert!(program_uses_curl(&parse("<?php $ch = \\CURL_INIT();")));
        assert!(program_uses_curl(&parse(
            "<?php function f(\\curlhandle $c): bool { return true; }"
        )));
        assert!(program_uses_curl(&parse(
            "<?php $ch = \\Some\\curl_exec($h);"
        )));
        assert!(program_uses_curl(&parse("<?php $o = \\CURLOPT_URL;")));
    }

    /// An aliased import (`use CurlHandle as Ch;`) is detected through the import name:
    /// the later `Ch $c` carries only the alias, so without inspecting the import the
    /// program would be a false negative (no prelude injected).
    #[test]
    fn detects_aliased_use_import() {
        assert!(program_uses_curl(&parse(
            "<?php use CurlHandle as Ch; function f(Ch $c): bool { return true; }"
        )));
        assert!(program_uses_curl(&parse(
            "<?php use function curl_init as ci; $ch = ci();"
        )));
    }

    /// A reference nested inside a call argument and a function body is detected.
    #[test]
    fn detects_nested_reference() {
        assert!(program_uses_curl(&parse(
            r#"<?php function run() { return helper(curl_init("https://example.test")); }"#
        )));
    }

    /// A first-class callable (`curl_exec(...)`) is detected.
    #[test]
    fn detects_first_class_callable() {
        assert!(program_uses_curl(&parse(
            "<?php $f = curl_exec(...); $f($ch);"
        )));
    }

    /// User functions and constants that merely start with `curl` do NOT inject: the
    /// prelude pulls in the `elephc_curl` bridge and the managed native `curl` package,
    /// so a `curl`-shaped user name must not cost a program that link.
    #[test]
    fn ignores_curl_shaped_user_names() {
        assert!(!program_uses_curl(&parse(
            "<?php function curl_helper() { return 1; } echo curl_helper();"
        )));
        assert!(!program_uses_curl(&parse(
            "<?php $m = curl_multiplexer();"
        )));
        assert!(!program_uses_curl(&parse("<?php echo curling();")));
        assert!(!program_uses_curl(&parse("<?php echo CURLY;")));
    }

    /// Mentions of the names only inside string literals and variable names do not
    /// trigger detection — the precision win over a `Debug`-string scan.
    #[test]
    fn ignores_non_call_mentions() {
        assert!(!program_uses_curl(&parse(
            r#"<?php $curlNote = "call curl_init first"; echo $curlNote;"#
        )));
    }

    /// A program with no curl mention at all is not detected.
    #[test]
    fn ignores_unrelated_program() {
        assert!(!program_uses_curl(&parse(
            "<?php $sum = 0; for ($i = 0; $i < 10; $i++) { $sum += $i; } echo $sum;"
        )));
    }
}
