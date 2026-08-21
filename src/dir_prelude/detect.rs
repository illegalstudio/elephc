//! Purpose:
//! Decides whether a parsed program references PHP's `dir()` / `Directory` surface, so the
//! prelude is injected only for programs that open a directory object — and whether the program
//! already declares its own `dir` function or `Directory` class, so a user definition is never
//! clobbered with a redeclaration error.
//!
//! Called from:
//! - `crate::dir_prelude::inject_if_used`.
//!
//! Key details:
//! - Runs before name resolution, so `Name`s are raw source text and PHP function and class names
//!   are case-insensitive. A reference may be written `dir`, `\dir`, or `\Some\dir`, and
//!   `Directory` may be spelled `\Directory` or `directory`. The walk therefore matches the
//!   unqualified last segment case-insensitively.
//! - THE FUNCTION SET IS EXACT, NOT A PREFIX. `dir` is a three-letter name with several ordinary
//!   builtin neighbours — `dirname`, `is_dir`, `opendir`, `readdir`, `scandir` — none of which
//!   needs the prelude. Whole-segment equality keeps every one of those programs untouched.
//! - `dir` is also a plausible USER function name and `Directory` a very plausible user class
//!   name, far more so than `hash_init`/`HashContext`. `program_declares_directory` therefore
//!   suppresses injection when the program defines either itself, the way the `var_export`
//!   prelude already does — otherwise adding the prelude would turn a working program into
//!   "Cannot redeclare".
//! - Soundness over precision: a missed reference would drop the prelude and turn a valid program
//!   into an "undefined function/class" error, so the `match`es are exhaustive (no wildcard arm).
//!   Adding an AST node forces this file to be updated. False positives only inject declarations,
//!   which is harmless — the prelude carries no executable top-level code.

use crate::names::Name;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    InstanceOfTarget, PackedField, StaticReceiver, Stmt, StmtKind, TraitAdaptation, TraitUse,
    TypeExpr,
};

/// The PHP functions the directory prelude declares. Exactly one, and matched as a whole segment:
/// `dirname`, `is_dir`, `opendir`, `readdir`, `rewinddir`, `closedir` and `scandir` are ordinary
/// native builtins that must not pull the prelude in.
const DIRECTORY_FUNCTIONS: &[&str] = &["dir"];

/// The single OOP class the directory prelude declares.
const DIRECTORY_CLASS: &str = "Directory";

/// Returns whether the program already declares its own `dir` function or `Directory` class, in
/// which case the prelude must not be injected so the user definition wins.
///
/// Both names are ordinary enough that a program may well own them — this check is why injecting
/// the prelude cannot turn a working program into a redeclaration error.
pub(super) fn program_declares_directory(program: &[Stmt]) -> bool {
    program.iter().any(stmt_declares_directory)
}

/// Returns whether a statement declares a `dir` function or a `Directory` class, recursing only
/// into the block forms that can host a hoisted declaration.
fn stmt_declares_directory(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::FunctionDecl { name, .. } => name.eq_ignore_ascii_case("dir"),
        StmtKind::ClassDecl { name, .. }
        | StmtKind::InterfaceDecl { name, .. }
        | StmtKind::TraitDecl { name, .. }
        | StmtKind::EnumDecl { name, .. } => name.eq_ignore_ascii_case(DIRECTORY_CLASS),
        StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => body.iter().any(stmt_declares_directory),
        _ => false,
    }
}

/// Returns whether any top-level statement references `dir()` or `Directory`, so the
/// prelude must be injected ahead of user code.
pub(super) fn program_uses_directory(program: &[Stmt]) -> bool {
    program.iter().any(stmt_refs_directory)
}

/// Returns whether `name`'s unqualified last segment is `dir`, compared case-insensitively
/// to match PHP's case-insensitive function names and any namespace/leading-backslash form
/// (`dir`, `\dir`, `\Some\dir`). Whole-segment equality, never a prefix — `dirname()` and
/// `is_dir()` are ordinary builtins that must not pull the prelude in.
fn name_is_directory_function(name: &Name) -> bool {
    name.last_segment().is_some_and(|segment| {
        DIRECTORY_FUNCTIONS
            .iter()
            .any(|candidate| segment.eq_ignore_ascii_case(candidate))
    })
}

/// Returns whether `name`'s unqualified last segment is `Directory`, compared
/// case-insensitively and tolerant of any namespace/leading-backslash form
/// (`Directory`, `\Directory`, `\Foo\Directory`).
fn name_is_directory_class(name: &Name) -> bool {
    name.last_segment()
        .is_some_and(|segment| segment.eq_ignore_ascii_case(DIRECTORY_CLASS))
}

/// Returns whether a static receiver names the class (`Directory::...`). `self`, `static`,
/// and `parent` never resolve to `Directory` at this position.
fn receiver_refs_directory(receiver: &StaticReceiver) -> bool {
    matches!(receiver, StaticReceiver::Named(name) if name_is_directory_class(name))
}

/// Returns whether an `instanceof` target references `Directory`, recursing into the
/// operand when the target is a runtime expression.
fn instanceof_target_refs_directory(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(name) => name_is_directory_class(name),
        InstanceOfTarget::Expr(expr) => expr_refs_directory(expr),
    }
}

/// Returns whether a first-class-callable target references the surface: the free function
/// (`dir(...)`), a static-method receiver, or an instance-method object expression.
fn callable_target_refs_directory(target: &CallableTarget) -> bool {
    match target {
        CallableTarget::Function(name) => name_is_directory_function(name),
        CallableTarget::StaticMethod { receiver, .. } => receiver_refs_directory(receiver),
        CallableTarget::Method { object, .. } => expr_refs_directory(object),
    }
}

/// Returns whether a type expression names `Directory`, recursing through
/// nullable/union/array/buffer wrappers and `ptr<Class>` targets.
fn type_refs_directory(type_expr: &TypeExpr) -> bool {
    match type_expr {
        TypeExpr::Int
        | TypeExpr::Float
        | TypeExpr::Bool
        | TypeExpr::False
        | TypeExpr::Str
        | TypeExpr::Void
        | TypeExpr::Never
        | TypeExpr::Iterable => false,
        TypeExpr::Ptr(target) => target.as_ref().is_some_and(name_is_directory_class),
        TypeExpr::Array(inner) | TypeExpr::Buffer(inner) | TypeExpr::Nullable(inner) => {
            type_refs_directory(inner)
        }
        TypeExpr::Named(name) => name_is_directory_class(name),
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            members.iter().any(type_refs_directory)
        }
    }
}

/// Returns whether any parameter's type hint or default value references the surface.
/// Shared by function, method, and closure parameter lists.
fn params_ref_directory(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)]) -> bool {
    params.iter().any(|(_, type_expr, default, _)| {
        type_expr.as_ref().is_some_and(type_refs_directory)
            || default.as_ref().is_some_and(expr_refs_directory)
    })
}

/// Returns whether a `use Trait` clause names `Directory` through its trait list or any
/// conflict-resolution adaptation.
fn trait_use_refs_directory(trait_use: &TraitUse) -> bool {
    trait_use.trait_names.iter().any(name_is_directory_class)
        || trait_use.adaptations.iter().any(|adaptation| match adaptation {
            TraitAdaptation::Alias { trait_name, .. } => {
                trait_name.as_ref().is_some_and(name_is_directory_class)
            }
            TraitAdaptation::InsteadOf {
                trait_name,
                instead_of,
                ..
            } => {
                trait_name.as_ref().is_some_and(name_is_directory_class)
                    || instead_of.iter().any(name_is_directory_class)
            }
        })
}

/// Returns whether a class property's type hint or default value references the surface.
fn class_property_refs_directory(property: &ClassProperty) -> bool {
    property.type_expr.as_ref().is_some_and(type_refs_directory)
        || property.default.as_ref().is_some_and(expr_refs_directory)
}

/// Returns whether a method's parameters, return type, or body reference the surface.
fn class_method_refs_directory(method: &ClassMethod) -> bool {
    params_ref_directory(&method.params)
        || method.return_type.as_ref().is_some_and(type_refs_directory)
        || method.body.iter().any(stmt_refs_directory)
}

/// Returns whether a class constant's initializer references the surface.
fn class_const_refs_directory(constant: &ClassConst) -> bool {
    expr_refs_directory(&constant.value)
}

/// Returns whether an enum case's backing-value expression references the surface.
fn enum_case_refs_directory(case: &EnumCaseDecl) -> bool {
    case.value.as_ref().is_some_and(expr_refs_directory)
}

/// Returns whether a `packed class` field's type references `Directory`. It is never a
/// valid packed field type, but the field is walked for completeness.
fn packed_field_refs_directory(field: &PackedField) -> bool {
    type_refs_directory(&field.type_expr)
}

/// Returns whether an expression references the surface at any function-call or class-name
/// position, recursing into every child expression and statement. The `match` is exhaustive
/// so a newly added `ExprKind` cannot silently bypass detection.
fn expr_refs_directory(expr: &Expr) -> bool {
    match &expr.kind {
        // Leaves and identifier-only forms carry no reference.
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
        | ExprKind::ConstRef(_)
        | ExprKind::MagicConstant(_) => false,

        ExprKind::BinaryOp { left, right, .. } => expr_refs_directory(left) || expr_refs_directory(right),
        ExprKind::InstanceOf { value, target } => {
            expr_refs_directory(value) || instanceof_target_refs_directory(target)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::YieldFrom(inner) => expr_refs_directory(inner),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_refs_directory(value) || expr_refs_directory(default)
        }
        ExprKind::Pipe { value, callable } => expr_refs_directory(value) || expr_refs_directory(callable),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            expr_refs_directory(target)
                || expr_refs_directory(value)
                || result_target.as_deref().is_some_and(expr_refs_directory)
                || prelude.iter().any(stmt_refs_directory)
        }
        // A free-function call is the dominant position: `dir()` is the only way to obtain
        // a `Directory` in the first place, so a program may use one without ever naming
        // the class.
        ExprKind::FunctionCall { name, args } => {
            name_is_directory_function(name) || args.iter().any(expr_refs_directory)
        }
        ExprKind::ClosureCall { args, .. } => args.iter().any(expr_refs_directory),
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_refs_directory),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs
            .iter()
            .any(|(key, value)| expr_refs_directory(key) || expr_refs_directory(value)),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_refs_directory(subject)
                || arms.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_directory) || expr_refs_directory(body)
                })
                || default.as_deref().is_some_and(expr_refs_directory)
        }
        ExprKind::ArrayAccess { array, index } => expr_refs_directory(array) || expr_refs_directory(index),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => expr_refs_directory(condition) || expr_refs_directory(then_expr) || expr_refs_directory(else_expr),
        ExprKind::Cast { expr, .. } | ExprKind::PtrCast { expr, .. } => expr_refs_directory(expr),
        ExprKind::Closure {
            params,
            return_type,
            body,
            ..
        } => {
            params_ref_directory(params)
                || return_type.as_ref().is_some_and(type_refs_directory)
                || body.iter().any(stmt_refs_directory)
        }
        ExprKind::NamedArg { value, .. } => expr_refs_directory(value),
        ExprKind::ExprCall { callee, args } => {
            expr_refs_directory(callee) || args.iter().any(expr_refs_directory)
        }
        ExprKind::NewObject { class_name, args } => {
            name_is_directory_class(class_name) || args.iter().any(expr_refs_directory)
        }
        ExprKind::NewDynamic { name_expr, args } => {
            expr_refs_directory(name_expr) || args.iter().any(expr_refs_directory)
        }
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class,
            required_parent,
            args,
        } => {
            expr_refs_directory(class_name)
                || name_is_directory_class(fallback_class)
                || name_is_directory_class(required_parent)
                || args.iter().any(expr_refs_directory)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_refs_directory(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_refs_directory(object) || expr_refs_directory(property)
        }
        ExprKind::StaticPropertyAccess { receiver, .. } => receiver_refs_directory(receiver),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_refs_directory(object) || args.iter().any(expr_refs_directory)
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => expr_refs_directory(object) || expr_refs_directory(method) || args.iter().any(expr_refs_directory),
        ExprKind::StaticMethodCall { receiver, args, .. } => {
            receiver_refs_directory(receiver) || args.iter().any(expr_refs_directory)
        }
        ExprKind::FirstClassCallable(target) => callable_target_refs_directory(target),
        ExprKind::BufferNew { element_type, len } => {
            type_refs_directory(element_type) || expr_refs_directory(len)
        }
        ExprKind::ClassConstant { receiver }
        | ExprKind::ScopedConstantAccess { receiver, .. } => receiver_refs_directory(receiver),
        ExprKind::ObjectClassName { object } => expr_refs_directory(object),
        ExprKind::NewScopedObject { receiver, args } => {
            receiver_refs_directory(receiver) || args.iter().any(expr_refs_directory)
        }
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr_refs_directory)
                || value.as_deref().is_some_and(expr_refs_directory)
        }
        // Transient: the resolver expands this into the included file's statements before
        // detection runs, so it should never reach here. Recurse into the path expression
        // defensively to keep detection exhaustive and correct.
        ExprKind::IncludeValue { path, .. } => expr_refs_directory(path),
    }
}

/// Returns whether a statement references the surface at any function-call or class-name
/// position, recursing into nested statements, expressions, and class members. The `match`
/// is exhaustive so a newly added `StmtKind` cannot silently bypass detection.
fn stmt_refs_directory(stmt: &Stmt) -> bool {
    match &stmt.kind {
        // Statements with no name position and no child expr/stmt.
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

        // An aliased import (`use Directory as D;`) names the class only here; the later
        // `D $d` carries the alias, which the walk cannot otherwise connect back — so
        // skipping imports would be a false negative. Function imports
        // (`use function dir as d;`) land in the same list and are caught by the same
        // check, which is why both name tests are applied.
        StmtKind::UseDecl { imports } => imports
            .iter()
            .any(|item| name_is_directory_class(&item.name) || name_is_directory_function(&item.name)),

        StmtKind::Echo(expr) | StmtKind::Throw(expr) | StmtKind::ExprStmt(expr) => {
            expr_refs_directory(expr)
        }
        StmtKind::Assign { value, .. } => expr_refs_directory(value),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_refs_directory(condition)
                || then_body.iter().any(stmt_refs_directory)
                || elseif_clauses
                    .iter()
                    .any(|(cond, body)| expr_refs_directory(cond) || body.iter().any(stmt_refs_directory))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_directory))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_refs_directory)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_directory))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_refs_directory(condition) || body.iter().any(stmt_refs_directory)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_refs_directory)
                || condition.as_ref().is_some_and(expr_refs_directory)
                || update.as_deref().is_some_and(stmt_refs_directory)
                || body.iter().any(stmt_refs_directory)
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            expr_refs_directory(index) || expr_refs_directory(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_refs_directory(target) || expr_refs_directory(value)
        }
        StmtKind::ArrayPush { value, .. } => expr_refs_directory(value),
        StmtKind::TypedAssign {
            type_expr, value, ..
        } => type_refs_directory(type_expr) || expr_refs_directory(value),
        StmtKind::Foreach { array, body, .. } => {
            expr_refs_directory(array) || body.iter().any(stmt_refs_directory)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_refs_directory(subject)
                || cases.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_directory) || body.iter().any(stmt_refs_directory)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_directory))
        }
        StmtKind::Include { path, .. } => expr_refs_directory(path),
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => body.iter().any(stmt_refs_directory),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_refs_directory)
                || catches.iter().any(|catch| {
                    catch.exception_types.iter().any(name_is_directory_class)
                        || catch.body.iter().any(stmt_refs_directory)
                })
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_directory))
        }
        StmtKind::FunctionDecl {
            params,
            return_type,
            body,
            ..
        } => {
            params_ref_directory(params)
                || return_type.as_ref().is_some_and(type_refs_directory)
                || body.iter().any(stmt_refs_directory)
        }
        StmtKind::Return(value) => value.as_ref().is_some_and(expr_refs_directory),
        StmtKind::ConstDecl { value, .. } => expr_refs_directory(value),
        StmtKind::ListUnpack { value, .. } => expr_refs_directory(value),
        StmtKind::StaticVar { init, .. } => expr_refs_directory(init),
        StmtKind::ClassDecl {
            extends,
            implements,
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.as_ref().is_some_and(name_is_directory_class)
                || implements.iter().any(name_is_directory_class)
                || trait_uses.iter().any(trait_use_refs_directory)
                || properties.iter().any(class_property_refs_directory)
                || methods.iter().any(class_method_refs_directory)
                || constants.iter().any(class_const_refs_directory)
        }
        StmtKind::EnumDecl {
            backing_type,
            cases,
            ..
        } => {
            backing_type.as_ref().is_some_and(type_refs_directory)
                || cases.iter().any(enum_case_refs_directory)
        }
        StmtKind::PackedClassDecl { fields, .. } => fields.iter().any(packed_field_refs_directory),
        StmtKind::InterfaceDecl {
            extends,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.iter().any(name_is_directory_class)
                || properties.iter().any(class_property_refs_directory)
                || methods.iter().any(class_method_refs_directory)
                || constants.iter().any(class_const_refs_directory)
        }
        StmtKind::TraitDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            trait_uses.iter().any(trait_use_refs_directory)
                || properties.iter().any(class_property_refs_directory)
                || methods.iter().any(class_method_refs_directory)
                || constants.iter().any(class_const_refs_directory)
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_refs_directory(object) || expr_refs_directory(value)
        }
        StmtKind::StaticPropertyAssign {
            receiver, value, ..
        }
        | StmtKind::StaticPropertyArrayPush {
            receiver, value, ..
        } => receiver_refs_directory(receiver) || expr_refs_directory(value),
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            index,
            value,
            ..
        } => receiver_refs_directory(receiver) || expr_refs_directory(index) || expr_refs_directory(value),
        StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_refs_directory(object) || expr_refs_directory(value)
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => expr_refs_directory(object) || expr_refs_directory(index) || expr_refs_directory(value),
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the `dir()`/`Directory` AST walk: the call and every `Directory` class-name
    //! position are detected across `\`-qualified and mixed-case spellings, while the neighbouring
    //! directory builtins are not, and a program that declares its own `dir`/`Directory` suppresses
    //! injection.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests parse raw source (pre name-resolution), matching the stage at which
    //!   `program_uses_directory` runs inside `inject_if_used`.

    use super::*;

    /// Parses source the same way `inject_if_used` sees it: tokenize then parse, before any name
    /// resolution.
    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// `dir(...)` — the only way to mint a `Directory` — is detected as a call.
    #[test]
    fn detects_dir_call() {
        assert!(program_uses_directory(&parse(r#"<?php $d = dir("/tmp");"#)));
    }

    /// A `Directory` parameter type hint is detected as a class reference, even when the body
    /// never calls `dir()`.
    #[test]
    fn detects_class_type_hint() {
        assert!(program_uses_directory(&parse(
            "<?php function f(Directory $d): bool { return true; }"
        )));
    }

    /// An `instanceof Directory` check is detected.
    #[test]
    fn detects_instanceof() {
        assert!(program_uses_directory(&parse(
            "<?php if ($x instanceof Directory) { echo 1; }"
        )));
    }

    /// A fully-qualified, differently-cased reference is detected on both the function side and
    /// the class side (last segment, case-insensitive).
    #[test]
    fn detects_fully_qualified_and_case_insensitive() {
        assert!(program_uses_directory(&parse(r#"<?php $d = \DIR("/tmp");"#)));
        assert!(program_uses_directory(&parse(
            "<?php function f(\\directory $d): bool { return true; }"
        )));
    }

    /// An aliased import (`use Directory as D;`) is detected through the import name: the later
    /// `D $d` carries only the alias, so without inspecting the import the program would be a
    /// false negative.
    #[test]
    fn detects_aliased_use_import() {
        assert!(program_uses_directory(&parse(
            "<?php use Directory as D; function f(D $d): bool { return true; }"
        )));
    }

    /// A reference nested inside a call argument and a function body is detected.
    #[test]
    fn detects_nested_reference() {
        assert!(program_uses_directory(&parse(
            r#"<?php function run() { return helper(dir("/tmp")); }"#
        )));
    }

    /// The neighbouring directory builtins stay native: none of them injects the prelude. This is
    /// the reason the function set is matched as a whole segment rather than by a `dir` substring.
    #[test]
    fn ignores_neighbouring_directory_builtins() {
        assert!(!program_uses_directory(&parse(
            r#"<?php echo dirname("/a/b");"#
        )));
        assert!(!program_uses_directory(&parse(
            r#"<?php var_dump(is_dir("/tmp"));"#
        )));
        assert!(!program_uses_directory(&parse(
            r#"<?php $h = opendir("/tmp"); readdir($h); rewinddir($h); closedir($h);"#
        )));
        assert!(!program_uses_directory(&parse(
            r#"<?php print_r(scandir("/tmp"));"#
        )));
    }

    /// Mentions of the names only inside string literals and variable names do not trigger
    /// detection.
    #[test]
    fn ignores_non_call_mentions() {
        assert!(!program_uses_directory(&parse(
            r#"<?php $dirNote = "call dir first"; echo $dirNote;"#
        )));
    }

    /// A program with no directory mention at all is not detected.
    #[test]
    fn ignores_unrelated_program() {
        assert!(!program_uses_directory(&parse(
            "<?php $sum = 0; for ($i = 0; $i < 10; $i++) { $sum += $i; } echo $sum;"
        )));
    }

    /// A program that declares its own `dir()` or `Directory` suppresses injection, so the user
    /// definition wins instead of colliding with the prelude's.
    #[test]
    fn detects_user_declarations_that_suppress_injection() {
        assert!(program_declares_directory(&parse(
            "<?php function dir($p) { return $p; }"
        )));
        assert!(program_declares_directory(&parse(
            "<?php class Directory { public $path = 1; }"
        )));
        assert!(!program_declares_directory(&parse(
            r#"<?php $d = dir("/tmp");"#
        )));
    }
}
