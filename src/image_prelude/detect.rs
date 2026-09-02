//! Purpose:
//! Decides whether a parsed program references any PHP image symbol — a GD/Exif/
//! IPTC procedural function or one of the image OOP classes (GdImage, Imagick,
//! Gmagick, Cairo, …) — so the image prelude is injected only for image-using
//! programs.
//!
//! Called from:
//! - `crate::image_prelude::inject_if_used`.
//!
//! Key details:
//! - Runs before name resolution, so `Name`s are raw source text and PHP function
//!   and class names are case-insensitive. The walk matches the unqualified last
//!   segment case-insensitively.
//! - Two kinds of positions trigger injection: (a) a *function call* whose name is
//!   an image function — detected by the `image`/`exif_`/`iptc`/`cairo_` prefix plus
//!   the handful of non-prefixed names (`getimagesize`, `getimagesizefromstring`,
//!   `read_exif_data`, `gd_info`); and (b) a *class-name* position naming an image
//!   class (new, type hints, `instanceof`, `catch`, `extends`/`implements`, trait
//!   uses, `use` imports). This mirrors `pdo_prelude::detect` but adds the
//!   procedural-call check, since image use is dominated by free functions.
//! - A user declaration or canonical `if (!function_exists(...))` polyfill owns
//!   its image-shaped function name. Such calls do not enable the image bridge;
//!   unrelated image calls and image-class references in the same program still do.
//! - Soundness over precision: a missed reference would drop the prelude and turn
//!   a valid program into an "undefined function/class" error, so the `match`es
//!   are exhaustive (no wildcard arm). Adding an AST node forces this file to be
//!   updated. False positives (e.g. a user function named `imageHelper`) only
//!   over-link the bridge harmlessly — the prelude carries only declarations.

use crate::names::Name;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    InstanceOfTarget, PackedField, StaticReceiver, Stmt, StmtKind, TraitAdaptation, TraitUse,
    TypeExpr,
};

/// Image OOP class names across GD, Imagick, Gmagick, and Cairo. Forward-declared
/// in full so detection covers every image class; a referenced class the prelude
/// does not provide produces a normal "class not found" error if referenced.
const IMAGE_CLASSES: &[&str] = &[
    "GdImage",
    "ImageException",
    "Imagick",
    "ImagickDraw",
    "ImagickPixel",
    "ImagickPixelIterator",
    "ImagickKernel",
    "ImagickException",
    "ImagickDrawException",
    "ImagickPixelException",
    "ImagickPixelIteratorException",
    "Gmagick",
    "GmagickDraw",
    "GmagickPixel",
    "GmagickException",
    "GmagickDrawException",
    "GmagickPixelException",
    "CairoContext",
    "CairoSurface",
    "CairoImageSurface",
    "CairoPdfSurface",
    "CairoPsSurface",
    "CairoSvgSurface",
    "CairoMatrix",
    "CairoPattern",
    "CairoSolidPattern",
    "CairoSurfacePattern",
    "CairoGradientPattern",
    "CairoLinearGradient",
    "CairoRadialGradient",
    "CairoFontFace",
    "CairoToyFontFace",
    "CairoFontOptions",
    "CairoScaledFont",
    "CairoPath",
    "CairoException",
];

/// Returns whether `s` begins with `prefix`, compared ASCII-case-insensitively.
fn starts_with_ci(s: &str, prefix: &str) -> bool {
    s.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// Returns whether any top-level statement references an image symbol, so the
/// prelude must be injected ahead of user code.
pub(super) fn program_uses_image(program: &[Stmt]) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    let image_function_references = usage
        .functions
        .iter()
        .filter(|name| string_is_image_function(name))
        .collect::<Vec<_>>();
    let all_image_functions_are_user_owned = !image_function_references.is_empty()
        && image_function_references.iter().all(|name| {
            crate::opcache_prelude::detect::program_declares(program, name)
                || program_has_guarded_polyfill(program, name)
        });
    let references_image_class = usage.classes.iter().any(|name| string_is_image_class(name));
    if all_image_functions_are_user_owned && !references_image_class {
        return false;
    }
    program.iter().any(stmt_refs_image)
}

/// Returns whether a normalized or source-spelled function name belongs to the image surface.
fn string_is_image_function(name: &str) -> bool {
    let name = name.rsplit('\\').next().unwrap_or(name);
    starts_with_ci(name, "image")
        || starts_with_ci(name, "exif_")
        || starts_with_ci(name, "iptc")
        || starts_with_ci(name, "cairo_")
        || name.eq_ignore_ascii_case("getimagesize")
        || name.eq_ignore_ascii_case("getimagesizefromstring")
        || name.eq_ignore_ascii_case("read_exif_data")
        || name.eq_ignore_ascii_case("gd_info")
}

/// Returns whether a normalized or source-spelled class name belongs to the image surface.
fn string_is_image_class(name: &str) -> bool {
    let name = name.rsplit('\\').next().unwrap_or(name);
    IMAGE_CLASSES
        .iter()
        .any(|class| name.eq_ignore_ascii_case(class))
}

/// Returns the literal function name tested by a canonical `!function_exists(...)` guard.
fn missing_function_guard_target(condition: &Expr) -> Option<&str> {
    let ExprKind::Not(inner) = &condition.kind else {
        return None;
    };
    let ExprKind::FunctionCall { name, args } = &inner.kind else {
        return None;
    };
    if !name
        .as_canonical()
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("function_exists")
    {
        return None;
    }
    match args.as_slice() {
        [Expr {
            kind: ExprKind::StringLiteral(target),
            ..
        }] => Some(target.trim_start_matches('\\')),
        _ => None,
    }
}

/// Finds a canonical guarded user polyfill for `target` through declaration-transparent blocks.
fn program_has_guarded_polyfill(program: &[Stmt], target: &str) -> bool {
    program.iter().any(|stmt| match &stmt.kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            missing_function_guard_target(condition).is_some_and(|guarded| {
                guarded.eq_ignore_ascii_case(target)
                    && crate::opcache_prelude::detect::program_declares(then_body, target)
            }) || program_has_guarded_polyfill(then_body, target)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| program_has_guarded_polyfill(body, target))
                || else_body
                    .as_ref()
                    .is_some_and(|body| program_has_guarded_polyfill(body, target))
        }
        StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body) => program_has_guarded_polyfill(body, target),
        _ => false,
    })
}

/// Activates guarded image polyfills after the detector decides the image capability is absent.
pub(super) fn activate_guarded_image_polyfills(program: Vec<Stmt>) -> Vec<Stmt> {
    program
        .into_iter()
        .flat_map(|mut stmt| {
            let guarded_target = match &stmt.kind {
                StmtKind::If {
                    condition,
                    then_body,
                    ..
                } => missing_function_guard_target(condition).filter(|target| {
                    string_is_image_function(target)
                        && crate::opcache_prelude::detect::program_declares(then_body, target)
                }),
                _ => None,
            };
            if guarded_target.is_some() {
                let StmtKind::If { then_body, .. } = &mut stmt.kind else {
                    unreachable!("guarded target only comes from an if statement");
                };
                return activate_guarded_image_polyfills(std::mem::take(then_body));
            }
            match &mut stmt.kind {
                StmtKind::NamespaceBlock { body, .. }
                | StmtKind::IncludeOnceGuard { body, .. }
                | StmtKind::Synthetic(body) => {
                    *body = activate_guarded_image_polyfills(std::mem::take(body));
                }
                _ => {}
            }
            vec![stmt]
        })
        .collect()
}

/// Returns whether `name`'s last segment is an image *function*: any GD function
/// (and `image_type_to_*`) via the `image` prefix, the Exif/IPTC families via
/// their prefixes, the procedural `cairo_*` family via the `cairo_` prefix,
/// plus the non-prefixed core/alias names.
fn name_is_image_function(name: &Name) -> bool {
    name.last_segment().is_some_and(string_is_image_function)
}

/// Returns whether `name`'s last segment is one of the image OOP classes,
/// compared case-insensitively and tolerant of any namespace/leading-backslash
/// form (`Imagick`, `\Imagick`, `\Foo\Imagick`).
fn name_is_image_class(name: &Name) -> bool {
    name.last_segment().is_some_and(string_is_image_class)
}

/// Returns whether a static receiver names an image class (`Imagick::...`).
fn receiver_refs_image(receiver: &StaticReceiver) -> bool {
    matches!(receiver, StaticReceiver::Named(name) if name_is_image_class(name))
}

/// Returns whether an `instanceof` target references an image class, recursing
/// into the operand when the target is a runtime expression.
fn instanceof_target_refs_image(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(name) => name_is_image_class(name),
        InstanceOfTarget::Expr(expr) => expr_refs_image(expr),
    }
}

/// Returns whether a first-class-callable target references an image symbol: a
/// free function (`imagepng(...)`), a static-method receiver, or an
/// instance-method object expression.
fn callable_target_refs_image(target: &CallableTarget) -> bool {
    match target {
        CallableTarget::Function(name) => name_is_image_function(name),
        CallableTarget::StaticMethod { receiver, .. } => receiver_refs_image(receiver),
        CallableTarget::Method { object, .. } => expr_refs_image(object),
    }
}

/// Returns whether a type expression names an image class, recursing through
/// nullable/union/array/buffer wrappers and `ptr<Class>` targets.
fn type_refs_image(type_expr: &TypeExpr) -> bool {
    match type_expr {
        TypeExpr::Int
        | TypeExpr::Float
        | TypeExpr::Bool
        | TypeExpr::False
        | TypeExpr::Str
        | TypeExpr::Void
        | TypeExpr::Never
        | TypeExpr::Iterable => false,
        TypeExpr::Ptr(target) => target.as_ref().is_some_and(name_is_image_class),
        TypeExpr::Array(inner) | TypeExpr::Buffer(inner) | TypeExpr::Nullable(inner) => {
            type_refs_image(inner)
        }
        TypeExpr::Named(name) => name_is_image_class(name),
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            members.iter().any(type_refs_image)
        }
    }
}

/// Returns whether any parameter's type hint or default value references an image
/// class. Shared by function, method, and closure parameter lists.
fn params_ref_image(params: &[(String, Option<TypeExpr>, Option<Expr>, bool)]) -> bool {
    params.iter().any(|(_, type_expr, default, _)| {
        type_expr.as_ref().is_some_and(type_refs_image)
            || default.as_ref().is_some_and(expr_refs_image)
    })
}

/// Returns whether a `use Trait` clause names an image class through its trait
/// list or any conflict-resolution adaptation.
fn trait_use_refs_image(trait_use: &TraitUse) -> bool {
    trait_use.trait_names.iter().any(name_is_image_class)
        || trait_use.adaptations.iter().any(|adaptation| match adaptation {
            TraitAdaptation::Alias { trait_name, .. } => {
                trait_name.as_ref().is_some_and(name_is_image_class)
            }
            TraitAdaptation::InsteadOf {
                trait_name,
                instead_of,
                ..
            } => {
                trait_name.as_ref().is_some_and(name_is_image_class)
                    || instead_of.iter().any(name_is_image_class)
            }
        })
}

/// Returns whether a class property's type hint or default value references an
/// image class.
fn class_property_refs_image(property: &ClassProperty) -> bool {
    property.type_expr.as_ref().is_some_and(type_refs_image)
        || property.default.as_ref().is_some_and(expr_refs_image)
}

/// Returns whether a method's parameters, return type, or body reference an image
/// symbol.
fn class_method_refs_image(method: &ClassMethod) -> bool {
    params_ref_image(&method.params)
        || method.return_type.as_ref().is_some_and(type_refs_image)
        || method.body.iter().any(stmt_refs_image)
}

/// Returns whether a class constant's initializer references an image symbol.
fn class_const_refs_image(constant: &ClassConst) -> bool {
    expr_refs_image(&constant.value)
}

/// Returns whether an enum case's backing-value expression references an image
/// symbol.
fn enum_case_refs_image(case: &EnumCaseDecl) -> bool {
    case.value.as_ref().is_some_and(expr_refs_image)
}

/// Returns whether a `packed class` field's type references an image class. Image
/// classes are never valid packed field types, but the field is walked for
/// completeness.
fn packed_field_refs_image(field: &PackedField) -> bool {
    type_refs_image(&field.type_expr)
}

/// Returns whether an expression references an image symbol at any function-call
/// or class-name position, recursing into every child expression and statement.
/// The `match` is exhaustive so a newly added `ExprKind` cannot silently bypass
/// detection.
fn expr_refs_image(expr: &Expr) -> bool {
    match &expr.kind {
        // Leaves and identifier-only forms carry no image reference.
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

        ExprKind::BinaryOp { left, right, .. } => expr_refs_image(left) || expr_refs_image(right),
        ExprKind::InstanceOf { value, target } => {
            expr_refs_image(value) || instanceof_target_refs_image(target)
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::YieldFrom(inner) => expr_refs_image(inner),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_refs_image(value) || expr_refs_image(default)
        }
        ExprKind::Pipe { value, callable } => expr_refs_image(value) || expr_refs_image(callable),
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            expr_refs_image(target)
                || expr_refs_image(value)
                || result_target.as_deref().is_some_and(expr_refs_image)
                || prelude.iter().any(stmt_refs_image)
        }
        // A free-function call is the dominant image-use position.
        ExprKind::FunctionCall { name, args } => {
            name_is_image_function(name) || args.iter().any(expr_refs_image)
        }
        ExprKind::ClosureCall { args, .. } => args.iter().any(expr_refs_image),
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_refs_image),
        ExprKind::ArrayLiteralAssoc(pairs) => pairs
            .iter()
            .any(|(key, value)| expr_refs_image(key) || expr_refs_image(value)),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_refs_image(subject)
                || arms.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_image) || expr_refs_image(body)
                })
                || default.as_deref().is_some_and(expr_refs_image)
        }
        ExprKind::ArrayAccess { array, index } => expr_refs_image(array) || expr_refs_image(index),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_refs_image(condition) || expr_refs_image(then_expr) || expr_refs_image(else_expr)
        }
        ExprKind::Cast { expr, .. } | ExprKind::PtrCast { expr, .. } => expr_refs_image(expr),
        ExprKind::Closure {
            params,
            return_type,
            body,
            ..
        } => {
            params_ref_image(params)
                || return_type.as_ref().is_some_and(type_refs_image)
                || body.iter().any(stmt_refs_image)
        }
        ExprKind::NamedArg { value, .. } => expr_refs_image(value),
        ExprKind::ExprCall { callee, args } => {
            expr_refs_image(callee) || args.iter().any(expr_refs_image)
        }
        ExprKind::NewObject { class_name, args } => {
            name_is_image_class(class_name) || args.iter().any(expr_refs_image)
        }
        ExprKind::NewDynamic { name_expr, args } => {
            expr_refs_image(name_expr) || args.iter().any(expr_refs_image)
        }
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class,
            required_parent,
            args,
        } => {
            expr_refs_image(class_name)
                || name_is_image_class(fallback_class)
                || name_is_image_class(required_parent)
                || args.iter().any(expr_refs_image)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_refs_image(object),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_refs_image(object) || expr_refs_image(property)
        }
        ExprKind::StaticPropertyAccess { receiver, .. } => receiver_refs_image(receiver),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_refs_image(object) || args.iter().any(expr_refs_image)
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => expr_refs_image(object) || expr_refs_image(method) || args.iter().any(expr_refs_image),
        ExprKind::StaticMethodCall { receiver, args, .. } => {
            receiver_refs_image(receiver) || args.iter().any(expr_refs_image)
        }
        ExprKind::FirstClassCallable(target) => callable_target_refs_image(target),
        ExprKind::BufferNew { element_type, len } => {
            type_refs_image(element_type) || expr_refs_image(len)
        }
        ExprKind::ClassConstant { receiver }
        | ExprKind::ScopedConstantAccess { receiver, .. } => receiver_refs_image(receiver),
        ExprKind::ObjectClassName { object } => expr_refs_image(object),
        ExprKind::NewScopedObject { receiver, args } => {
            receiver_refs_image(receiver) || args.iter().any(expr_refs_image)
        }
        ExprKind::Yield { key, value } => {
            key.as_deref().is_some_and(expr_refs_image)
                || value.as_deref().is_some_and(expr_refs_image)
        }
        // Transient: the resolver expands this into the included file's statements
        // before image detection runs, so it should never reach here. Recurse into
        // the path expression defensively to keep detection exhaustive and correct.
        ExprKind::IncludeValue { path, .. } => expr_refs_image(path),
    }
}

/// Returns whether a statement references an image symbol at any function-call or
/// class-name position, recursing into nested statements, expressions, and class
/// members. The `match` is exhaustive so a newly added `StmtKind` cannot silently
/// bypass detection.
fn stmt_refs_image(stmt: &Stmt) -> bool {
    match &stmt.kind {
        // Statements with no image-name position and no child expr/stmt.
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

        // An aliased import (`use Imagick as Im;`) names the class only here.
        StmtKind::UseDecl { imports } => imports.iter().any(|item| name_is_image_class(&item.name)),

        StmtKind::Echo(expr) | StmtKind::Throw(expr) | StmtKind::ExprStmt(expr) => {
            expr_refs_image(expr)
        }
        StmtKind::Assign { value, .. } => expr_refs_image(value),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expr_refs_image(condition)
                || then_body.iter().any(stmt_refs_image)
                || elseif_clauses
                    .iter()
                    .any(|(cond, body)| expr_refs_image(cond) || body.iter().any(stmt_refs_image))
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_image))
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_refs_image)
                || else_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_image))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            expr_refs_image(condition) || body.iter().any(stmt_refs_image)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_refs_image)
                || condition.as_ref().is_some_and(expr_refs_image)
                || update.as_deref().is_some_and(stmt_refs_image)
                || body.iter().any(stmt_refs_image)
        }
        StmtKind::ArrayAssign { index, value, .. } => {
            expr_refs_image(index) || expr_refs_image(value)
        }
        StmtKind::NestedArrayAssign { target, value } => {
            expr_refs_image(target) || expr_refs_image(value)
        }
        StmtKind::ArrayPush { value, .. } => expr_refs_image(value),
        StmtKind::TypedAssign {
            type_expr, value, ..
        } => type_refs_image(type_expr) || expr_refs_image(value),
        StmtKind::Foreach { array, body, .. } => {
            expr_refs_image(array) || body.iter().any(stmt_refs_image)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            expr_refs_image(subject)
                || cases.iter().any(|(conditions, body)| {
                    conditions.iter().any(expr_refs_image) || body.iter().any(stmt_refs_image)
                })
                || default
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_image))
        }
        StmtKind::Include { path, .. } => expr_refs_image(path),
        StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. } => body.iter().any(stmt_refs_image),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            try_body.iter().any(stmt_refs_image)
                || catches.iter().any(|catch| {
                    catch.exception_types.iter().any(name_is_image_class)
                        || catch.body.iter().any(stmt_refs_image)
                })
                || finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(stmt_refs_image))
        }
        StmtKind::FunctionDecl {
            params,
            return_type,
            body,
            ..
        } => {
            params_ref_image(params)
                || return_type.as_ref().is_some_and(type_refs_image)
                || body.iter().any(stmt_refs_image)
        }
        StmtKind::Return(value) => value.as_ref().is_some_and(expr_refs_image),
        StmtKind::ConstDecl { value, .. } => expr_refs_image(value),
        StmtKind::ListUnpack { value, .. } => expr_refs_image(value),
        StmtKind::StaticVar { init, .. } => expr_refs_image(init),
        StmtKind::ClassDecl {
            extends,
            implements,
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.as_ref().is_some_and(name_is_image_class)
                || implements.iter().any(name_is_image_class)
                || trait_uses.iter().any(trait_use_refs_image)
                || properties.iter().any(class_property_refs_image)
                || methods.iter().any(class_method_refs_image)
                || constants.iter().any(class_const_refs_image)
        }
        StmtKind::EnumDecl {
            backing_type,
            cases,
            ..
        } => {
            backing_type.as_ref().is_some_and(type_refs_image)
                || cases.iter().any(enum_case_refs_image)
        }
        StmtKind::PackedClassDecl { fields, .. } => fields.iter().any(packed_field_refs_image),
        StmtKind::InterfaceDecl {
            extends,
            properties,
            methods,
            constants,
            ..
        } => {
            extends.iter().any(name_is_image_class)
                || properties.iter().any(class_property_refs_image)
                || methods.iter().any(class_method_refs_image)
                || constants.iter().any(class_const_refs_image)
        }
        StmtKind::TraitDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            trait_uses.iter().any(trait_use_refs_image)
                || properties.iter().any(class_property_refs_image)
                || methods.iter().any(class_method_refs_image)
                || constants.iter().any(class_const_refs_image)
        }
        StmtKind::PropertyAssign { object, value, .. } => {
            expr_refs_image(object) || expr_refs_image(value)
        }
        StmtKind::StaticPropertyAssign {
            receiver, value, ..
        }
        | StmtKind::StaticPropertyArrayPush {
            receiver, value, ..
        } => receiver_refs_image(receiver) || expr_refs_image(value),
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            index,
            value,
            ..
        } => receiver_refs_image(receiver) || expr_refs_image(index) || expr_refs_image(value),
        StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_refs_image(object) || expr_refs_image(value)
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => expr_refs_image(object) || expr_refs_image(index) || expr_refs_image(value),
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the image-usage AST walk: procedural calls and class-name
    //! positions are detected across PHP and `\`-qualified/case spellings, while
    //! non-call/non-class mentions (string literals, variables) are not.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests parse raw source (pre name-resolution), matching the stage at which
    //!   `program_uses_image` runs inside `inject_if_used`.

    use super::*;

    /// Parses source the same way `inject_if_used` sees it: tokenize then parse,
    /// before any name resolution.
    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// A GD procedural call (`imagecreatetruecolor`) is detected by prefix.
    #[test]
    fn detects_gd_function_call() {
        assert!(program_uses_image(&parse(
            "<?php $im = imagecreatetruecolor(8, 8);"
        )));
    }

    /// A canonical guarded user polyfill does not opt the program into the image bridge.
    #[test]
    fn ignores_guarded_image_function_polyfill() {
        assert!(!program_uses_image(&parse(
            "<?php if (!function_exists('imagecreatetruecolor')) { function imagecreatetruecolor(int $w, int $h): string { return 'polyfill'; } } echo imagecreatetruecolor(8, 8);"
        )));
    }

    /// Another unshadowed image function still enables the bridge beside a guarded polyfill.
    #[test]
    fn detects_unshadowed_image_call_beside_polyfill() {
        assert!(program_uses_image(&parse(
            "<?php if (!function_exists('imagecreatetruecolor')) { function imagecreatetruecolor(int $w, int $h): string { return 'polyfill'; } } echo imagecreatetruecolor(8, 8); imagepng($im);"
        )));
    }

    /// Activating the absent-capability branch exposes its function declaration to later passes.
    #[test]
    fn activates_guarded_image_function_polyfill() {
        let program = activate_guarded_image_polyfills(parse(
            "<?php if (!function_exists('imagecreatetruecolor')) { function imagecreatetruecolor(int $w, int $h): string { return 'polyfill'; } } echo imagecreatetruecolor(8, 8);"
        ));
        assert!(crate::opcache_prelude::detect::program_declares(
            &program,
            "imagecreatetruecolor"
        ));
    }

    /// The non-prefixed core function `getimagesize` is detected.
    #[test]
    fn detects_getimagesize() {
        assert!(program_uses_image(&parse(
            r#"<?php $info = getimagesize("a.png");"#
        )));
    }

    /// An Exif call (`exif_read_data`) is detected by prefix.
    #[test]
    fn detects_exif_call() {
        assert!(program_uses_image(&parse(
            r#"<?php $d = exif_read_data("a.jpg");"#
        )));
    }

    /// A procedural cairo call (`cairo_image_surface_create`) is detected by the
    /// `cairo_` prefix, so a program using only the functional layer still pulls
    /// in the prelude that defines those wrappers.
    #[test]
    fn detects_cairo_procedural_call() {
        assert!(program_uses_image(&parse(
            r#"<?php $s = cairo_image_surface_create(0, 10, 10);"#
        )));
    }

    /// A `new GdImage(...)` / type-hint position is detected as a class reference.
    #[test]
    fn detects_image_class_type_hint() {
        assert!(program_uses_image(&parse(
            "<?php function f(GdImage $im): int { return imagesx($im); }"
        )));
    }

    /// A fully-qualified, differently-cased class reference is detected.
    #[test]
    fn detects_fully_qualified_imagick() {
        assert!(program_uses_image(&parse(
            r#"<?php $m = new \imagick("a.png");"#
        )));
    }

    /// Mentions of "image" only inside string literals and variable names do not
    /// trigger detection — the precision win over a `Debug`-string scan.
    #[test]
    fn ignores_non_call_mentions() {
        assert!(!program_uses_image(&parse(
            r#"<?php $imageNote = "render the image"; echo $imageNote;"#
        )));
    }

    /// A program with no image mention at all is not detected.
    #[test]
    fn ignores_unrelated_program() {
        assert!(!program_uses_image(&parse(
            "<?php $sum = 0; for ($i = 0; $i < 10; $i++) { $sum += $i; } echo $sum;"
        )));
    }
}
