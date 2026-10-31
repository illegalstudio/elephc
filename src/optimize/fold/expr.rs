//! Purpose:
//! Implements constant-folding support for expr expressions.
//! Evaluates compile-time scalar cases that are safe to replace with literal AST nodes.
//!
//! Called from:
//! - `crate::optimize::fold`
//!
//! Key details:
//! - Folding must respect PHP coercions, truthiness, numeric edge cases, and runtime error boundaries.

use super::super::*;
use super::super::{fold_block, try_prune_match_expr};
use super::casts::try_fold_cast;
use super::ops::{
    try_fold_array_access, try_fold_binary_op, try_fold_bit_not, try_fold_negate, try_fold_not,
    try_fold_null_coalesce, try_fold_short_ternary, try_fold_ternary,
};

/// Folds default expressions in function parameters.
/// Returns a new parameter list with each parameter's default expression folded.
pub(in crate::optimize) fn fold_params(
    params: Vec<(
        String,
        Option<crate::parser::ast::TypeExpr>,
        Option<Expr>,
        bool,
    )>,
) -> Vec<(
    String,
    Option<crate::parser::ast::TypeExpr>,
    Option<Expr>,
    bool,
)> {
    params
        .into_iter()
        .map(|(name, type_expr, default, is_ref)| (name, type_expr, default.map(fold_expr), is_ref))
        .collect()
}

/// Folds default expressions in a class property declaration.
pub(in crate::optimize) fn fold_property(property: ClassProperty) -> ClassProperty {
    ClassProperty {
        name: property.name,
        visibility: property.visibility,
        set_visibility: property.set_visibility,
        type_expr: property.type_expr,
        hooks: property.hooks,
        readonly: property.readonly,
        is_final: property.is_final,
        is_static: property.is_static,
        is_abstract: property.is_abstract,
        by_ref: property.by_ref,
        is_promoted: property.is_promoted,
        default: property.default.map(fold_expr),
        span: property.span,
        attributes: property.attributes,
    }
}

/// Folds default expressions and block body in a class method declaration.
pub(in crate::optimize) fn fold_method(method: ClassMethod) -> ClassMethod {
    let body = super::super::target_guards::fold_callable_body(
        method.body,
        method.params.iter().filter(|(_, _, _, by_ref)| *by_ref)
            .map(|(name, _, _, _)| name.as_str()),
    );
    ClassMethod {
        name: method.name,
        // A REBUILD. Folding rewrites a body, never a declaration's type parameters.
        type_params: method.type_params,
        visibility: method.visibility,
        is_static: method.is_static,
        is_abstract: method.is_abstract,
        is_final: method.is_final,
        has_body: method.has_body,
        params: fold_params(method.params),
        param_attributes: method.param_attributes,
        variadic: method.variadic,
        variadic_by_ref: method.variadic_by_ref,
        variadic_type: method.variadic_type,
        return_type: method.return_type,
        by_ref_return: method.by_ref_return,
        body,
        span: method.span,
        attributes: method.attributes,
    }
}

/// Folds the optional value expression in an enum case declaration.
pub(in crate::optimize) fn fold_enum_case(case: EnumCaseDecl) -> EnumCaseDecl {
    EnumCaseDecl {
        name: case.name,
        value: case.value.map(fold_expr),
        span: case.span,
        attributes: case.attributes,
    }
}

/// Recursively folds constant expressions in an AST expression.
///
/// Dispatches each `ExprKind` variant to the appropriate helper under `ops`,
/// `casts`, `pipes`, or `inline_closure`. Returns the folded expression, or
/// falls back to the original node when no fold is applicable. Division by
/// zero, overflow, and other PHP-runtime behaviors are intentionally left as
/// runtime decisions — this function only folds when the result is an
/// unambiguous PHP equivalent.
pub(in crate::optimize) fn fold_expr(expr: Expr) -> Expr {
    let span = expr.span;
    let kind = match expr.kind {
        // `IncludeValue` is a transient parser node fully expanded by the resolver;
        // it can never reach this pass.
        ExprKind::IncludeValue { .. } => {
            unreachable!("ExprKind::IncludeValue must be expanded by the resolver")
        }
        ExprKind::StringLiteral(value) => ExprKind::StringLiteral(value),
        ExprKind::IntLiteral(value) => ExprKind::IntLiteral(value),
        ExprKind::FloatLiteral(value) => ExprKind::FloatLiteral(value),
        ExprKind::Variable(name) => ExprKind::Variable(name),
        ExprKind::BinaryOp { left, op, right } => {
            let left = fold_expr(*left);
            let right = fold_expr(*right);
            try_fold_binary_op(&op, &left, &right).unwrap_or_else(|| ExprKind::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            })
        }
        ExprKind::InstanceOf { value, target } => ExprKind::InstanceOf {
            value: Box::new(fold_expr(*value)),
            target: fold_instanceof_target(target),
        },
        ExprKind::BoolLiteral(value) => ExprKind::BoolLiteral(value),
        ExprKind::Null => ExprKind::Null,
        ExprKind::Negate(inner) => {
            let inner = fold_expr(*inner);
            try_fold_negate(&inner).unwrap_or_else(|| ExprKind::Negate(Box::new(inner)))
        }
        ExprKind::Not(inner) => {
            let inner = fold_expr(*inner);
            try_fold_not(&inner).unwrap_or_else(|| ExprKind::Not(Box::new(inner)))
        }
        ExprKind::BitNot(inner) => {
            let inner = fold_expr(*inner);
            try_fold_bit_not(&inner).unwrap_or_else(|| ExprKind::BitNot(Box::new(inner)))
        }
        ExprKind::Throw(inner) => ExprKind::Throw(Box::new(fold_expr(*inner))),
        ExprKind::Clone(inner) => ExprKind::Clone(Box::new(fold_expr(*inner))),
        ExprKind::ErrorSuppress(inner) => ExprKind::ErrorSuppress(Box::new(fold_expr(*inner))),
        ExprKind::Print(inner) => ExprKind::Print(Box::new(fold_expr(*inner))),
        ExprKind::NullCoalesce { value, default } => {
            let value = fold_expr(*value);
            let default = fold_expr(*default);
            try_fold_null_coalesce(&value, &default).unwrap_or_else(|| ExprKind::NullCoalesce {
                value: Box::new(value),
                default: Box::new(default),
            })
        }
        ExprKind::Pipe { value, callable } => {
            let value = fold_expr(*value);
            let callable = fold_expr(*callable);
            super::pipes::try_fold_pure_pipe(&value, &callable)
                .or_else(|| super::inline_closure::try_inline_closure_pipe(&value, &callable))
                .unwrap_or_else(|| ExprKind::Pipe {
                    value: Box::new(value),
                    callable: Box::new(callable),
                })
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp,
        } => {
            let target = Box::new(fold_expr(*target));
            let value = Box::new(fold_expr(*value));
            let result_target = result_target
                .map(|inner| Box::new(fold_expr(*inner)))
                .filter(|inner| inner.kind != target.kind);
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude: fold_block(prelude),
                conditional_value_temp,
            }
        }
        ExprKind::PreIncrement(name) => ExprKind::PreIncrement(name),
        ExprKind::PostIncrement(name) => ExprKind::PostIncrement(name),
        ExprKind::PreDecrement(name) => ExprKind::PreDecrement(name),
        ExprKind::PostDecrement(name) => ExprKind::PostDecrement(name),
        ExprKind::FunctionCall { name, args } => {
            let args = args.into_iter().map(fold_expr).collect::<Vec<_>>();
            if let Some(rebound) = namespace_fallbacks::resolve_call(&name, &args, span) {
                return fold_expr(rebound);
            }
            if let Some(folded) = try_fold_scalar_builtin(&name, &args) {
                return fold_expr(Expr::new(folded, span));
            }
            if name
                .as_canonical()
                .trim_start_matches('\\')
                .eq_ignore_ascii_case("function_exists")
            {
                if let ([Expr { kind: ExprKind::StringLiteral(candidate), .. }], Some(target)) =
                    (args.as_slice(), active_fold_target())
                {
                    let candidate = candidate.trim_start_matches('\\');
                    let date_alias = crate::name_resolver::is_global_date_procedural_alias(candidate);
                    if crate::builtins::registry::lookup(candidate).is_some() || date_alias {
                        let user_function = active_fold_user_function_exists(candidate);
                        let profile_builtin = crate::types::checker::builtins::is_php_visible_builtin_function_for_profile(
                            candidate,
                            crate::strict_php::is_enabled(),
                        );
                        if user_function || profile_builtin || date_alias {
                            let target_builtin = crate::types::checker::builtins::is_php_visible_builtin_function_for_target(
                                candidate,
                                crate::strict_php::is_enabled(),
                                target,
                            );
                            if user_function || target_builtin || date_alias || active_target_guard_condition() {
                                ExprKind::BoolLiteral(user_function || target_builtin || date_alias)
                            } else {
                                ExprKind::FunctionCall { name, args }
                            }
                        } else {
                            ExprKind::FunctionCall { name, args }
                        }
                    } else {
                        ExprKind::FunctionCall { name, args }
                    }
                } else {
                    ExprKind::FunctionCall { name, args }
                }
            } else {
                ExprKind::FunctionCall { name, args }
            }
        }
        ExprKind::ArrayLiteral(items) => {
            ExprKind::ArrayLiteral(items.into_iter().map(fold_expr).collect())
        }
        ExprKind::ArrayLiteralAssoc(items) => ExprKind::ArrayLiteralAssoc(
            items
                .into_iter()
                .map(|(key, value)| (fold_expr(key), fold_expr(value)))
                .collect(),
        ),
        ExprKind::ArrayLiteralMixed(entries) => {
            let literal = ExprKind::ArrayLiteralMixed(entries.into_iter()
                .map(|entry| entry.map_exprs(fold_expr)).collect());
            crate::types::fold_spreadless_mixed_array_literal(
                &literal, crate::codegen_support::compile_php_version(),
            ).unwrap_or(literal)
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            let subject = fold_expr(*subject);
            let arms = arms
                .into_iter()
                .map(|(patterns, value)| {
                    (
                        patterns.into_iter().map(fold_expr).collect(),
                        fold_expr(value),
                    )
                })
                .collect();
            let default = default.map(|expr| Box::new(fold_expr(*expr)));
            try_prune_match_expr(subject, arms, default)
        }
        ExprKind::ArrayAccess { array, index } => {
            let array = fold_expr(*array);
            let index = fold_expr(*index);
            try_fold_array_access(&array, &index).unwrap_or_else(|| ExprKind::ArrayAccess {
                array: Box::new(array),
                index: Box::new(index),
            })
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            let condition = fold_expr(*condition);
            let then_expr = fold_expr(*then_expr);
            let else_expr = fold_expr(*else_expr);
            try_fold_ternary(&condition, &then_expr, &else_expr).unwrap_or_else(|| {
                ExprKind::Ternary {
                    condition: Box::new(condition),
                    then_expr: Box::new(then_expr),
                    else_expr: Box::new(else_expr),
                }
            })
        }
        ExprKind::ShortTernary { value, default } => {
            let value = fold_expr(*value);
            let default = fold_expr(*default);
            try_fold_short_ternary(&value, &default).unwrap_or_else(|| ExprKind::ShortTernary {
                value: Box::new(value),
                default: Box::new(default),
            })
        }
        ExprKind::Cast { target, expr } => {
            let expr = fold_expr(*expr);
            try_fold_cast(&target, &expr).unwrap_or_else(|| ExprKind::Cast {
                target,
                expr: Box::new(expr),
            })
        }
        ExprKind::Closure {
            params,
            variadic,
            variadic_by_ref,
            variadic_type,
            return_type,
            body,
            is_arrow,
            is_static,
            captures,
            capture_refs,
            by_ref_return,
        } => {
            let body = super::super::target_guards::fold_callable_body(
                body,
                params.iter().filter(|(_, _, _, by_ref)| *by_ref)
                    .map(|(name, _, _, _)| name.as_str())
                    .chain(capture_refs.iter().map(String::as_str)),
            );
            ExprKind::Closure {
                params: fold_params(params),
                variadic,
                variadic_by_ref,
                variadic_type,
                return_type,
                body,
                is_arrow,
                is_static,
                captures,
                capture_refs,
                by_ref_return,
            }
        },
        ExprKind::NamedArg { name, value } => ExprKind::NamedArg {
            name,
            value: Box::new(fold_expr(*value)),
        },
        ExprKind::Spread(inner) => ExprKind::Spread(Box::new(fold_expr(*inner))),
        ExprKind::ClosureCall { var, args } => ExprKind::ClosureCall {
            var,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::ExprCall { callee, args } => ExprKind::ExprCall {
            callee: Box::new(fold_expr(*callee)),
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::ConstRef(name) => match (
            name.trim_start_matches('\\'),
            active_fold_target(),
        ) {
            ("PHP_OS", Some(target)) => {
                ExprKind::StringLiteral(target.platform.php_os_name().to_string())
            }
            ("PHP_OS_FAMILY", Some(target)) => {
                ExprKind::StringLiteral(target.platform.php_os_family_name().to_string())
            }
            _ => ExprKind::ConstRef(name),
        },
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::NewGeneric { class_type, args } => ExprKind::NewGeneric {
            class_type,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::NewDynamic { name_expr, args } => ExprKind::NewDynamic {
            name_expr: Box::new(fold_expr(*name_expr)),
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::NewDynamicObject {
            class_name,
            fallback_class,
            required_parent,
            args,
        } => ExprKind::NewDynamicObject {
            class_name: Box::new(fold_expr(*class_name)),
            fallback_class,
            required_parent,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(fold_expr(*object)),
            property,
        },
        ExprKind::DynamicPropertyAccess { object, property } => ExprKind::DynamicPropertyAccess {
            object: Box::new(fold_expr(*object)),
            property: Box::new(fold_expr(*property)),
        },
        ExprKind::NullsafePropertyAccess { object, property } => ExprKind::NullsafePropertyAccess {
            object: Box::new(fold_expr(*object)),
            property,
        },
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            ExprKind::NullsafeDynamicPropertyAccess {
                object: Box::new(fold_expr(*object)),
                property: Box::new(fold_expr(*property)),
            }
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            ExprKind::StaticPropertyAccess { receiver, property }
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => ExprKind::MethodCall {
            object: Box::new(fold_expr(*object)),
            method,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => ExprKind::NullsafeMethodCall {
            object: Box::new(fold_expr(*object)),
            method,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => ExprKind::NullsafeDynamicMethodCall {
            object: Box::new(fold_expr(*object)),
            method: Box::new(fold_expr(*method)),
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => ExprKind::StaticMethodCall {
            receiver,
            method,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::FirstClassCallable(target) => {
            ExprKind::FirstClassCallable(fold_callable_target(target))
        }
        ExprKind::This => ExprKind::This,
        ExprKind::PtrCast { target_type, expr } => ExprKind::PtrCast {
            target_type,
            expr: Box::new(fold_expr(*expr)),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type,
            len: Box::new(fold_expr(*len)),
        },
        // Namespace resolution has already fixed a named receiver's spelling. Lexical
        // and late-static class names still depend on their declaration or runtime scope.
        ExprKind::ClassConstant { receiver: crate::parser::ast::StaticReceiver::Named(name) } => {
            ExprKind::StringLiteral(crate::names::class_name_literal(name.as_str()))
        }
        ExprKind::ClassConstant { receiver } => ExprKind::ClassConstant { receiver },
        ExprKind::ObjectClassName { object } => ExprKind::ObjectClassName {
            object: Box::new(fold_expr(*object)),
        },
        ExprKind::ScopedConstantAccess { receiver, name } => {
            ExprKind::ScopedConstantAccess { receiver, name }
        }
        ExprKind::NewScopedObject { receiver, args } => ExprKind::NewScopedObject {
            receiver,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::Yield { key, value } => ExprKind::Yield {
            key: key.map(|k| Box::new(fold_expr(*k))),
            value: value.map(|v| Box::new(fold_expr(*v))),
        },
        ExprKind::YieldFrom(inner) => ExprKind::YieldFrom(Box::new(fold_expr(*inner))),
        ExprKind::MagicConstant(_) => {
            unreachable!("MagicConstant must be lowered before optimizer passes")
        }
    };
    Expr { kind, span }
}

/// Folds the target of an instanceof expression, recursing into the expression form.
fn fold_instanceof_target(target: InstanceOfTarget) -> InstanceOfTarget {
    match target {
        InstanceOfTarget::Name(name) => InstanceOfTarget::Name(name),
        // Folding runs BEFORE instantiation, so the arguments have to survive it untouched.
        InstanceOfTarget::Generic(class_type) => InstanceOfTarget::Generic(class_type),
        InstanceOfTarget::Expr(expr) => InstanceOfTarget::Expr(Box::new(fold_expr(*expr))),
    }
}

/// Folds the target of a first-class callable, recursing into object expressions.
fn fold_callable_target(target: CallableTarget) -> CallableTarget {
    match target {
        CallableTarget::Function(name) => CallableTarget::Function(namespace_fallbacks::resolve_name(name)),
        CallableTarget::StaticMethod { receiver, method } => {
            CallableTarget::StaticMethod { receiver, method }
        }
        CallableTarget::Method { object, method } => CallableTarget::Method {
            object: Box::new(fold_expr(*object)),
            method,
        },
    }
}

/// Folds a one-argument `intval`/`floatval`/`strval` call over a scalar literal.
///
/// The cast-fold helpers already reproduce PHP's numeric-prefix, overflow and scalar coercion
/// rules, so a literal argument takes exactly the result the runtime cast would produce. A user
/// function shadowing the global name, a namespaced call, or an explicit base argument keeps the
/// call for runtime evaluation.
fn try_fold_scalar_builtin(name: &Name, args: &[Expr]) -> Option<ExprKind> {
    let canonical = crate::names::php_symbol_key(name.as_canonical().trim_start_matches('\\'));
    if active_fold_user_function_exists(&canonical) {
        return None;
    }
    if let Some(folded) = try_fold_mb_literal(&canonical, args) {
        return Some(folded);
    }
    // `token_name()` only maps the stable tokenizer constant catalog. A literal identifier or
    // the matching `T_*` constant is therefore a compile-time lookup; punctuation and unknown
    // ids keep PHP's "UNKNOWN". Resolving `ConstRef` here also catches `token_name(T_ECHO)`,
    // whose constant is not lowered to an integer until after the first fold pass.
    if canonical == "token_name" && args.len() == 1 {
        let id = match &args[0].kind {
            ExprKind::IntLiteral(id) => Some(*id),
            ExprKind::ConstRef(name) => {
                token_constant_value(name.as_canonical().trim_start_matches('\\'))
            }
            _ => None,
        };
        if let Some(id) = id {
            let name = token_constant_name(id).unwrap_or("UNKNOWN");
            return Some(ExprKind::StringLiteral(name.to_string()));
        }
    }
    // A callback that always returns the same replacement is equivalent to the string form
    // when the closure has no captures or side effects and the replacement contains no
    // backreference syntax. Reusing `mb_ereg_replace()` also selects its native fast path.
    if canonical == "mb_ereg_replace_callback"
        && matches!(args.len(), 3 | 4)
        && !active_fold_user_function_exists("mb_ereg_replace")
    {
        if let Some(replacement) = constant_callback_replacement(&args[1]) {
            let mut rewritten = vec![
                args[0].clone(),
                Expr::new(ExprKind::StringLiteral(replacement.to_string()), args[1].span),
                args[2].clone(),
            ];
            if let Some(options) = args.get(3) {
                rewritten.push(options.clone());
            }
            return Some(ExprKind::FunctionCall {
                name: crate::names::Name::unqualified("mb_ereg_replace"),
                args: rewritten,
            });
        }
    }
    if args.len() != 1 {
        return None;
    }
    let target = match canonical.as_str() {
        "intval" => CastType::Int,
        "floatval" => CastType::Float,
        "strval" => CastType::String,
        _ => return None,
    };
    super::casts::try_fold_cast(&target, &args[0])
}

/// Returns the constant string returned by a capture-free, side-effect-free callback.
///
/// Backslashes and dollar signs are rejected because `mb_ereg_replace()` interprets them as
/// replacement backreferences, while the callback returns them literally.
fn constant_callback_replacement(callback: &Expr) -> Option<&str> {
    let ExprKind::Closure {
        body,
        captures,
        capture_refs,
        by_ref_return,
        ..
    } = &callback.kind
    else {
        return None;
    };
    if !captures.is_empty() || !capture_refs.is_empty() || *by_ref_return {
        return None;
    }
    let [
        crate::parser::ast::Stmt {
            kind: crate::parser::ast::StmtKind::Return(Some(value)),
            ..
        },
    ] = body.as_slice()
    else {
        return None;
    };
    let ExprKind::StringLiteral(replacement) = &value.kind else {
        return None;
    };
    if replacement.contains('\\') || replacement.contains('$') {
        return None;
    }
    Some(replacement)
}

/// Folds the small literal mbstring regex and MIME forms whose PHP result is deterministic.
///
/// The patterns are matched byte-for-byte and only ASCII subjects are accepted, so UTF-8 case
/// folding, multibyte regex semantics, and header folding never need to be reproduced here.
/// Unsupported patterns, dynamic arguments, and non-ASCII regex subjects stay on the bridge.
fn try_fold_mb_literal(canonical: &str, args: &[Expr]) -> Option<ExprKind> {
    let string = |index: usize| match args.get(index)?.kind {
        ExprKind::StringLiteral(ref value) => Some(value.as_str()),
        _ => None,
    };
    let int = |index: usize| match args.get(index)?.kind {
        ExprKind::IntLiteral(value) => Some(value),
        _ => None,
    };
    match canonical {
        // `iconv_strpos()` counts characters, so only ASCII operands keep the byte index equal
        // to the PHP-visible result. A literal zero offset avoids PHP's negative-offset and
        // out-of-range diagnostics; a miss stays on the runtime path so `false` is exact.
        "iconv_strpos" if args.len() == 4 => {
            let haystack = string(0)?;
            let needle = string(1)?;
            if int(2)? != 0
                || !string(3)?.eq_ignore_ascii_case("UTF-8")
                || !haystack.is_ascii()
                || !needle.is_ascii()
                || needle.is_empty()
            {
                return None;
            }
            haystack
                .find(needle)
                .map(|position| ExprKind::IntLiteral(position as i64))
        }
        // `iconv_strrpos()` has no offset; the same ASCII and encoding restrictions apply.
        "iconv_strrpos" if args.len() == 3 => {
            let haystack = string(0)?;
            let needle = string(1)?;
            if !string(2)?.eq_ignore_ascii_case("UTF-8")
                || !haystack.is_ascii()
                || !needle.is_ascii()
                || needle.is_empty()
            {
                return None;
            }
            haystack
                .rfind(needle)
                .map(|position| ExprKind::IntLiteral(position as i64))
        }
        "mb_decode_mimeheader" if args.len() == 1 => {
            let encoded = string(0)?;
            let decoded = decode_mime_b_word(encoded)?;
            Some(ExprKind::StringLiteral(decoded))
        }
        "mb_ereg" if args.len() == 2 => {
            if string(0)? != "[0-9]+" { return None; }
            let subject = string(1)?;
            Some(ExprKind::BoolLiteral(subject.bytes().any(|byte| byte.is_ascii_digit())))
        }
        "mb_ereg_match" if args.len() == 2 => {
            if string(0)? != "[0-9A-Za-z]+" { return None; }
            let subject = string(1)?;
            Some(ExprKind::BoolLiteral(subject.bytes().any(|byte| byte.is_ascii_alphanumeric())))
        }
        "mb_eregi" if args.len() == 2 => {
            if string(0)? != "[A-Z]+" { return None; }
            let subject = string(1)?;
            if !subject.is_ascii() { return None; }
            Some(ExprKind::BoolLiteral(subject.bytes().any(|byte| byte.is_ascii_alphabetic())))
        }
        "mb_eregi_replace" if args.len() == 3 => {
            if string(0)? != "[A-Z]+" { return None; }
            let replacement = string(1)?;
            let subject = string(2)?;
            if !subject.is_ascii() || replacement.bytes().any(|byte| matches!(byte, b'\\' | b'$')) {
                return None;
            }
            let mut output = String::with_capacity(subject.len());
            let mut in_run = false;
            for byte in subject.bytes() {
                if byte.is_ascii_alphabetic() {
                    if !in_run { output.push_str(replacement); in_run = true; }
                } else {
                    output.push(byte as char);
                    in_run = false;
                }
            }
            Some(ExprKind::StringLiteral(output))
        }
        _ => None,
    }
}

/// Decodes one RFC 2047 UTF-8 base64 encoded word, or returns `None` for any other form.
pub(crate) fn decode_mime_b_word(encoded: &str) -> Option<String> {
    const PREFIX: &str = "=?UTF-8?B?";
    if encoded.len() < PREFIX.len() + 2 { return None; }
    if !encoded.is_char_boundary(PREFIX.len()) || !encoded.is_char_boundary(encoded.len() - 2) {
        return None;
    }
    if !encoded[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) || !encoded[encoded.len() - 2..].eq_ignore_ascii_case("?=") {
        return None;
    }
    let payload = &encoded[PREFIX.len()..encoded.len() - 2];
    let bytes = decode_base64(payload)?;
    String::from_utf8(bytes).ok()
}

/// Decodes canonical base64 without whitespace, rejecting malformed padding.
fn decode_base64(payload: &str) -> Option<Vec<u8>> {
    let bytes = payload.as_bytes();
    if bytes.is_empty() || bytes.len() % 4 != 0 { return None; }
    let mut output = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let mut values = [0u8; 4];
        let mut padding = 0usize;
        for (index, &byte) in chunk.iter().enumerate() {
            values[index] = match byte {
                b'A'..=b'Z' => byte - b'A',
                b'a'..=b'z' => byte - b'a' + 26,
                b'0'..=b'9' => byte - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                b'=' => { padding += 1; 0 }
                _ => return None,
            };
            if byte == b'=' && index < 2 { return None; }
        }
        if padding == 2 && (chunk[2] != b'=' || chunk[3] != b'=') { return None; }
        if padding == 1 && chunk[3] != b'=' { return None; }
        output.push((values[0] << 2) | (values[1] >> 4));
        if padding < 2 { output.push((values[1] << 4) | (values[2] >> 2)); }
        if padding < 1 { output.push((values[2] << 6) | values[3]); }
    }
    Some(output)
}

/// Resolves a tokenizer constant name to its integer id in the shared contract catalog.
fn token_constant_value(name: &str) -> Option<i64> {
    use elephc_builtin_contract::{ConstValue, PhpModule};
    elephc_builtin_contract::constants()
        .iter()
        .find_map(|constant| {
            if constant.module != PhpModule::Tokenizer || constant.name != name {
                return None;
            }
            match constant.value {
                ConstValue::Int(value) => Some(value),
                _ => None,
            }
        })
}

/// Returns the canonical tokenizer name for a literal token id, matching the engine's last-wins
/// catalog insertion order and excluding the historical PAAMAYIM alias.
fn token_constant_name(id: i64) -> Option<&'static str> {
    use elephc_builtin_contract::{ConstValue, PhpModule};
    elephc_builtin_contract::constants()
        .iter()
        .rev()
        .find_map(|constant| {
            if constant.module != PhpModule::Tokenizer
                || !constant.name.starts_with("T_")
                || constant.name == "T_PAAMAYIM_NEKUDOTAYIM"
            {
                return None;
            }
            match constant.value {
                ConstValue::Int(value) if value == id => Some(constant.name),
                _ => None,
            }
        })
}
