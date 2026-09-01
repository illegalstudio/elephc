//! Purpose:
//! Resolves names embedded in expressions and callable targets.
//! Rewrites function, constant, class, method, enum, object, and instanceof references as needed.
//!
//! Called from:
//! - `crate::name_resolver::statements` and declaration resolvers.
//!
//! Key details:
//! - PHP builtin fallback applies to unqualified function calls without breaking explicit namespace references.

use crate::names::php_symbol_key;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, InstanceOfTarget, StaticReceiver};

use super::names::{
    resolve_constant_name, resolve_function_name, resolve_special_or_class_name,
    resolve_type_expr, resolved_class_constant_name,
};
use super::statements::{resolve_params, resolve_stmt_list};
use super::{resolved_name, rewrite_callback_literal_args, Imports, Symbols};

/// Wraps a deprecated predefined constant read in a typed runtime-diagnostic passthrough.
fn deprecated_constant_read(
    value: Expr,
    is_string: bool,
    message: String,
    line: u32,
) -> ExprKind {
    ExprKind::StaticMethodCall {
        receiver: StaticReceiver::Named(resolved_name("DateTime".to_string())),
        method: if is_string {
            "__elephc_deprecated_string_constant".to_string()
        } else {
            "__elephc_deprecated_int_constant".to_string()
        },
        args: vec![
            value,
            Expr::new(ExprKind::StringLiteral(message), crate::span::Span::dummy()),
            Expr::new(
                ExprKind::IntLiteral(line as i64),
                crate::span::Span::dummy(),
            ),
        ],
    }
}

/// Rewrites compiler-only synthetic method names in user source so they cannot
/// reach implementation helpers that php-src does not expose.
fn source_visible_method_name(method: &str) -> String {
    // The compiler-injected HashContext prelude calls this helper from its own
    // PHP body. It is already part of main's public prelude contract, so hiding
    // it here would rewrite the prelude itself and leave hash_init() unresolved.
    if method.eq_ignore_ascii_case("__elephc_wrap") {
        return method.to_string();
    }
    if method
        .get(.."__elephc_".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("__elephc_"))
    {
        format!("{method}__php_src_hidden")
    } else {
        method.to_string()
    }
}

/// Recursively resolves names in an expression, returning a new expression with
/// all name references rewritten according to namespace and import rules.
///
/// Handles function calls, class/constant references, instanceof targets, closures,
/// method calls, and all other expression variants. Unqualified names are resolved
/// against current_namespace and imports. PHP builtin fallback applies to function
/// names that remain unqualified after resolution.
pub(super) fn resolve_expr(
    expr: &Expr,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Expr {
    let kind = match &expr.kind {
        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
            left: Box::new(resolve_expr(left, current_namespace, imports, symbols)),
            op: op.clone(),
            right: Box::new(resolve_expr(right, current_namespace, imports, symbols)),
        },
        ExprKind::InstanceOf { value, target } => ExprKind::InstanceOf {
            value: Box::new(resolve_expr(value, current_namespace, imports, symbols)),
            target: resolve_instanceof_target(target, current_namespace, imports, symbols),
        },
        ExprKind::Throw(inner) => {
            ExprKind::Throw(Box::new(resolve_expr(inner, current_namespace, imports, symbols)))
        }
        ExprKind::Print(inner) => {
            ExprKind::Print(Box::new(resolve_expr(inner, current_namespace, imports, symbols)))
        }
        ExprKind::Not(inner) => {
            ExprKind::Not(Box::new(resolve_expr(inner, current_namespace, imports, symbols)))
        }
        ExprKind::Negate(inner) => {
            ExprKind::Negate(Box::new(resolve_expr(inner, current_namespace, imports, symbols)))
        }
        ExprKind::BitNot(inner) => {
            ExprKind::BitNot(Box::new(resolve_expr(inner, current_namespace, imports, symbols)))
        }
        ExprKind::ErrorSuppress(inner) => ExprKind::ErrorSuppress(Box::new(resolve_expr(
            inner,
            current_namespace,
            imports,
            symbols,
        ))),
        ExprKind::Clone(inner) => {
            ExprKind::Clone(Box::new(resolve_expr(inner, current_namespace, imports, symbols)))
        }
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(resolve_expr(value, current_namespace, imports, symbols)),
            default: Box::new(resolve_expr(default, current_namespace, imports, symbols)),
        },
        ExprKind::Pipe { value, callable } => ExprKind::Pipe {
            value: Box::new(resolve_expr(value, current_namespace, imports, symbols)),
            callable: Box::new(resolve_expr(callable, current_namespace, imports, symbols)),
        },
        ExprKind::FunctionCall { name, args } => {
            let function_name = resolve_function_name(name, current_namespace, imports, symbols);
            let resolved_args: Vec<Expr> = rewrite_callback_literal_args(
                &function_name,
                args,
                current_namespace,
                imports,
                symbols,
            )
            .into_iter()
            .map(|arg| resolve_expr(&arg, current_namespace, imports, symbols))
            .collect();
            // A compiler-owned CLI `ini_set()` call desugars to the dispatch helper carrying the
            // real source line. This arm runs before `declares_function` because the injected
            // public wrapper is itself a declaration; the helper marker is the stricter ownership
            // guard.
            if let Some(rewritten) =
                rewrite_cli_ini_set(&function_name, &resolved_args, expr.span, symbols)
            {
                rewritten
            }
            // `var_export($v, true)` / `var_export($v)` desugar to the prelude helper that
            // matches the mode's PHP return type exactly. This arm runs BEFORE the
            // `declares_function` guard below because the elephc prelude declares
            // `var_export` itself, so that guard would always block it; the rewrite carries
            // its own, stricter guard instead (see `rewrite_var_export_return_flag`).
            else if let Some(rewritten) =
                rewrite_var_export_return_flag(&function_name, &resolved_args, symbols)
            {
                rewritten
            }
            // Procedural date/time aliases desugar to the equivalent OOP construction or method
            // call (e.g. date_create($s) -> new DateTime($s), date_diff($a, $b) -> $a->diff($b)).
            // Skip the rewrite when the resolved name is a user-declared function, so a
            // user-defined (e.g. namespaced `App\date_diff`) call is never hijacked.
            else if symbols.declares_function(&function_name) {
                ExprKind::FunctionCall {
                    name: resolved_name(function_name),
                    args: resolved_args,
                }
            } else if let Some(rewritten) =
                rewrite_date_procedural_alias(&function_name, &resolved_args, expr.span)
            {
                rewritten
            } else {
                ExprKind::FunctionCall {
                    name: resolved_name(function_name),
                    args: resolved_args,
                }
            }
        }
        ExprKind::ArrayLiteral(values) => ExprKind::ArrayLiteral(
            values
                .iter()
                .map(|value| resolve_expr(value, current_namespace, imports, symbols))
                .collect(),
        ),
        ExprKind::ArrayLiteralAssoc(values) => ExprKind::ArrayLiteralAssoc(
            values
                .iter()
                .map(|(key, value)| {
                    (
                        resolve_expr(key, current_namespace, imports, symbols),
                        resolve_expr(value, current_namespace, imports, symbols),
                    )
                })
                .collect(),
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => ExprKind::Match {
            subject: Box::new(resolve_expr(subject, current_namespace, imports, symbols)),
            arms: arms
                .iter()
                .map(|(conds, value)| {
                    (
                        conds
                            .iter()
                            .map(|cond| resolve_expr(cond, current_namespace, imports, symbols))
                            .collect(),
                        resolve_expr(value, current_namespace, imports, symbols),
                    )
                })
                .collect(),
            default: default
                .as_ref()
                .map(|expr| Box::new(resolve_expr(expr, current_namespace, imports, symbols))),
        },
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(resolve_expr(array, current_namespace, imports, symbols)),
            index: Box::new(resolve_expr(index, current_namespace, imports, symbols)),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(resolve_expr(condition, current_namespace, imports, symbols)),
            then_expr: Box::new(resolve_expr(then_expr, current_namespace, imports, symbols)),
            else_expr: Box::new(resolve_expr(else_expr, current_namespace, imports, symbols)),
        },
        ExprKind::ShortTernary { value, default } => ExprKind::ShortTernary {
            value: Box::new(resolve_expr(value, current_namespace, imports, symbols)),
            default: Box::new(resolve_expr(default, current_namespace, imports, symbols)),
        },
        ExprKind::Cast { target, expr } => ExprKind::Cast {
            target: target.clone(),
            expr: Box::new(resolve_expr(expr, current_namespace, imports, symbols)),
        },
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
        } => ExprKind::Closure {
            params: resolve_params(params, current_namespace, imports, symbols),
            variadic: variadic.clone(),
            variadic_by_ref: *variadic_by_ref,
            variadic_type: variadic_type.clone(),
            return_type: return_type
                .as_ref()
                .map(|ty| resolve_type_expr(ty, current_namespace, imports, symbols)),
            body: resolve_stmt_list(body, current_namespace, imports, symbols)
                .expect("name resolver bug: closure body resolution failed"),
            is_arrow: *is_arrow,
            is_static: *is_static,
            by_ref_return: *by_ref_return,
            captures: captures.clone(),
            capture_refs: capture_refs.clone(),
        },
        ExprKind::Spread(inner) => {
            ExprKind::Spread(Box::new(resolve_expr(inner, current_namespace, imports, symbols)))
        }
        ExprKind::ClosureCall { var, args } => ExprKind::ClosureCall {
            var: var.clone(),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        ExprKind::ExprCall { callee, args } => ExprKind::ExprCall {
            callee: Box::new(resolve_expr(callee, current_namespace, imports, symbols)),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        ExprKind::ConstRef(name) => {
            let resolved =
                resolve_constant_name(name, current_namespace, imports, symbols);
            let value = Expr::new(
                ExprKind::ConstRef(resolved_name(resolved.clone())),
                expr.span,
            );
            let bare = resolved.trim_start_matches('\\');
            if bare.eq_ignore_ascii_case("E_STRICT")
                && crate::codegen_support::compile_php_version().version_id() >= 80400
            {
                deprecated_constant_read(
                    value,
                    false,
                    "\nDeprecated: Constant E_STRICT is deprecated since 8.4, the error level was removed".to_string(),
                    expr.span.line,
                )
            } else if bare.eq_ignore_ascii_case("DATE_RFC7231") {
                deprecated_constant_read(
                    value,
                    true,
                    "\nDeprecated: Constant DATE_RFC7231 is deprecated since 8.5, as this format ignores the associated timezone and always uses GMT".to_string(),
                    expr.span.line,
                )
            } else if [
                "SUNFUNCS_RET_TIMESTAMP",
                "SUNFUNCS_RET_STRING",
                "SUNFUNCS_RET_DOUBLE",
            ]
            .iter()
            .any(|constant| bare.eq_ignore_ascii_case(constant))
            {
                deprecated_constant_read(
                    value,
                    false,
                    format!(
                        "\nDeprecated: Constant {} is deprecated since 8.4, as date_sunrise() and date_sunset() were deprecated in 8.1",
                        bare
                    ),
                    expr.span.line,
                )
            } else {
                value.kind
            }
        }
        ExprKind::NewObject { class_name, args } => {
            let resolved_class = resolved_name(resolve_special_or_class_name(
                class_name,
                current_namespace,
                imports,
                symbols,
            ));
            let resolved_args: Vec<Expr> = args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect();
            // Keep DatePeriod's deprecated string overload as an object construction.
            // EIR lowering allocates the object first and invokes the hidden in-place
            // initializer, matching php-src's object-id order and constructor entry point.
            ExprKind::NewObject {
                class_name: resolved_class,
                args: resolved_args,
            }
        }
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
            property: property.clone(),
        },
        ExprKind::DynamicPropertyAccess { object, property } => {
            ExprKind::DynamicPropertyAccess {
                object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
                property: Box::new(resolve_expr(property, current_namespace, imports, symbols)),
            }
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            ExprKind::NullsafePropertyAccess {
                object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
                property: property.clone(),
            }
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            ExprKind::NullsafeDynamicPropertyAccess {
                object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
                property: Box::new(resolve_expr(property, current_namespace, imports, symbols)),
            }
        }
        ExprKind::StaticPropertyAccess { receiver, property } => ExprKind::StaticPropertyAccess {
            receiver: match receiver {
                StaticReceiver::Named(name) => StaticReceiver::Named(resolved_name(
                    resolve_special_or_class_name(name, current_namespace, imports, symbols),
                )),
                _ => receiver.clone(),
            },
            property: property.clone(),
        },
        ExprKind::ClassConstant { receiver } => ExprKind::ClassConstant {
            receiver: match receiver {
                StaticReceiver::Named(name) => StaticReceiver::Named(resolved_name(
                    resolved_class_constant_name(name, current_namespace, imports),
                )),
                _ => receiver.clone(),
            },
        },
        ExprKind::ObjectClassName { object } => ExprKind::ObjectClassName {
            object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
        },
        ExprKind::ScopedConstantAccess { receiver, name } => {
            let resolved_receiver = match receiver {
                StaticReceiver::Named(name) => StaticReceiver::Named(resolved_name(
                    resolve_special_or_class_name(name, current_namespace, imports, symbols),
                )),
                _ => receiver.clone(),
            };
            let value = Expr::new(
                ExprKind::ScopedConstantAccess {
                    receiver: resolved_receiver.clone(),
                    name: name.clone(),
                },
                expr.span,
            );
            let is_datetime_rfc7231 = name.eq_ignore_ascii_case("RFC7231")
                && matches!(
                    &resolved_receiver,
                    StaticReceiver::Named(class_name)
                        if class_name.last_segment().is_some_and(|segment| {
                            segment.eq_ignore_ascii_case("DateTimeInterface")
                                || segment.eq_ignore_ascii_case("DateTime")
                                || segment.eq_ignore_ascii_case("DateTimeImmutable")
                        })
                );
            if is_datetime_rfc7231 {
                deprecated_constant_read(
                    value,
                    true,
                    "\nDeprecated: Constant DateTimeInterface::RFC7231 is deprecated since 8.5, as this format ignores the associated timezone and always uses GMT".to_string(),
                    expr.span.line,
                )
            } else {
                value.kind
            }
        }
        ExprKind::NewScopedObject { receiver, args } => ExprKind::NewScopedObject {
            receiver: match receiver {
                StaticReceiver::Named(name) => StaticReceiver::Named(resolved_name(
                    resolve_special_or_class_name(name, current_namespace, imports, symbols),
                )),
                _ => receiver.clone(),
            },
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        ExprKind::MethodCall { object, method, args } => ExprKind::MethodCall {
            object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
            method: source_visible_method_name(method),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        ExprKind::NullsafeMethodCall { object, method, args } => ExprKind::NullsafeMethodCall {
            object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
            method: source_visible_method_name(method),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => ExprKind::NullsafeDynamicMethodCall {
            object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
            method: Box::new(resolve_expr(method, current_namespace, imports, symbols)),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => {
            let resolved_receiver = match receiver {
                StaticReceiver::Named(name) => StaticReceiver::Named(resolved_name(
                    resolve_special_or_class_name(name, current_namespace, imports, symbols),
                )),
                _ => receiver.clone(),
            };
            // Keep the source spelling: dispatch lookups fold case at lookup
            // time, and `__callStatic` must receive the as-written name.
            let method_key = php_symbol_key(method);
            let mut resolved_args: Vec<Expr> = args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect();
            let is_builtin_datetime_receiver = matches!(
                &resolved_receiver,
                StaticReceiver::Named(name)
                    if name.last_segment().is_some_and(|segment| {
                        segment.eq_ignore_ascii_case("DateTime")
                            || segment.eq_ignore_ascii_case("DateTimeImmutable")
                    })
                        && !symbols.declares_class_like(&name.as_canonical())
            );
            if method_key == "createfromformat" && is_builtin_datetime_receiver {
                coerce_create_from_format_datetime_arg(&mut resolved_args);
            }
            // DateTimeZone::listIdentifiers([$group[, $country]]) desugars to the
            // injected __elephc_list_identifiers free function (mirrors the
            // timezone_identifiers_list arm in rewrite_date_procedural_alias): a
            // function's flow-inferred array<string> return keeps its element type,
            // so in_array works on the filtered result, where the synthetic method
            // would yield scalar mixed and regress in_array.
            if method_key == "listidentifiers"
                && resolved_args.len() <= 2
                && matches!(
                    &resolved_receiver,
                    StaticReceiver::Named(name)
                        if name.last_segment().is_some_and(|seg| seg.eq_ignore_ascii_case("DateTimeZone"))
                            && !symbols.declares_class_like(&name.as_canonical())
                )
            {
                if resolved_args.is_empty() {
                    resolved_args.push(Expr::new(ExprKind::IntLiteral(2047), expr.span));
                }
                if resolved_args.len() == 1 {
                    resolved_args.push(Expr::new(
                        ExprKind::StringLiteral(String::new()),
                        expr.span,
                    ));
                }
                resolved_args.push(Expr::new(
                    ExprKind::StringLiteral("DateTimeZone::listIdentifiers".to_string()),
                    expr.span,
                ));
                ExprKind::FunctionCall {
                    name: resolved_name("__elephc_list_identifiers".to_string()),
                    args: resolved_args,
                }
            } else {
                ExprKind::StaticMethodCall {
                    receiver: resolved_receiver,
                    method: source_visible_method_name(method),
                    args: resolved_args,
                }
            }
        }
        ExprKind::FirstClassCallable(target) => ExprKind::FirstClassCallable(match target {
            CallableTarget::Function(name) => CallableTarget::Function(resolved_name(
                resolve_function_name(name, current_namespace, imports, symbols),
            )),
            CallableTarget::StaticMethod { receiver, method } => CallableTarget::StaticMethod {
                receiver: match receiver {
                    StaticReceiver::Named(name) => StaticReceiver::Named(resolved_name(
                        resolve_special_or_class_name(name, current_namespace, imports, symbols),
                    )),
                    _ => receiver.clone(),
                },
                method: php_symbol_key(&source_visible_method_name(method)),
            },
            CallableTarget::Method { object, method } => CallableTarget::Method {
                object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
                method: php_symbol_key(&source_visible_method_name(method)),
            },
        }),
        ExprKind::PtrCast { target_type, expr } => ExprKind::PtrCast {
            target_type: target_type.clone(),
            expr: Box::new(resolve_expr(expr, current_namespace, imports, symbols)),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type: resolve_type_expr(element_type, current_namespace, imports, symbols),
            len: Box::new(resolve_expr(len, current_namespace, imports, symbols)),
        },
        // A named argument wraps its value expression — without this arm the value escaped
        // resolution entirely, so e.g. `new self(url: new Url('/'))` left the imported `Url`
        // alias unresolved ("Undefined class: Url").
        ExprKind::NamedArg { name, value } => ExprKind::NamedArg {
            name: name.clone(),
            value: Box::new(resolve_expr(value, current_namespace, imports, symbols)),
        },
        _ => expr.kind.clone(),
    };
    Expr::new(kind, expr.span)
}

/// Resolves the target of an instanceof expression.
///
/// If the target is a bare name, it is rewritten using resolve_special_or_class_name
/// to apply namespace/use rules. Expression targets are recursively resolved.
fn resolve_instanceof_target(
    target: &InstanceOfTarget,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> InstanceOfTarget {
    match target {
        InstanceOfTarget::Name(name) => InstanceOfTarget::Name(resolved_name(
            resolve_special_or_class_name(name, current_namespace, imports, symbols),
        )),
        InstanceOfTarget::Expr(expr) => InstanceOfTarget::Expr(Box::new(resolve_expr(
            expr,
            current_namespace,
            imports,
            symbols,
        ))),
    }
}

/// Splits a `var_export` argument list into its `$value` and `$return` expressions, accepting
/// both the positional and the PHP 8 named-argument spellings.
///
/// Returns `None` for anything that is not a well-formed, STATICALLY COUNTABLE `var_export`
/// argument list (an unknown parameter name, a missing `$value`, more than two arguments, or a
/// `...$args` spread whose element count and `$return` value are unknown here), so the caller
/// leaves such a call untouched and the ordinary arity/type diagnostics still fire on the real
/// function.
fn var_export_call_arguments<'a>(args: &'a [Expr]) -> Option<(&'a Expr, Option<&'a Expr>)> {
    let mut value: Option<&Expr> = None;
    let mut flag: Option<&Expr> = None;
    let mut positional = 0usize;
    for arg in args {
        match &arg.kind {
            ExprKind::NamedArg { name, value: inner } => match name.as_str() {
                "value" if value.is_none() => value = Some(inner),
                "return" if flag.is_none() => flag = Some(inner),
                _ => return None,
            },
            // A spread contributes an unknown number of arguments, so neither the `$value`
            // position nor the `$return` flag can be read off the call site.
            ExprKind::Spread(_) => return None,
            _ => {
                match positional {
                    0 if value.is_none() => value = Some(arg),
                    1 if flag.is_none() => flag = Some(arg),
                    _ => return None,
                }
                positional += 1;
            }
        }
    }
    value.map(|value| (value, flag))
}

/// Rewrites a `var_export()` call whose `$return` flag is a literal onto the prelude helper whose
/// inferred return type matches that mode exactly, or returns `None` to leave the call alone.
///
/// WHY THIS EXISTS. `var_export` is injected as elephc-PHP (`crate::var_export_prelude`), and its
/// single body has to serve both PHP modes: `return $rendered;` in return mode and `return null;`
/// in echo mode. Since `wider_type` stopped resolving `Void` away (so an unhinted function that
/// can return null infers `Union([T, Void])`, which `?string` and ternary/match joins already
/// produced), the inferred type of EVERY `var_export` call became `string|null`. That is honest
/// for a runtime flag but wrong for a literal one, and it broke the common
/// `function f(): string { return var_export($x, true); }` with
/// "return type expects Str, got Union([Str, Void])".
///
/// This is the same flag-aware treatment `print_r` gets from
/// `crate::builtins::io::print_r::check`, applied one layer up because `var_export` is a PHP
/// function rather than a registry builtin: rather than *asserting* a narrower type at the call
/// site (which would contradict the boxed `Mixed` the callee actually returns), the call is
/// retargeted at a helper that genuinely returns that type.
///
/// - `var_export($v, true)` → `__elephc_var_export_str($v, 0)`, declared `: string`.
/// - `var_export($v)` / `var_export($v, false)` → `__elephc_var_export_echo($v)`, which prints and
///   returns `null` (elephc `Void`), matching reference PHP 8.5.6.
/// - A runtime `$return` flag keeps the real `var_export`, whose `Union([Str, Void])` IS the
///   honest `string|null` PHP documents; the mode is then selected at run time by the `if` in the
///   prelude body rather than guessed here.
///
/// GUARD, in two halves — the caller cannot use its usual `symbols.declares_function` check here,
/// because the prelude declares `var_export` itself.
///
/// 1. The name must have resolved to the GLOBAL `var_export`, i.e. carry no namespace qualifier.
///    Matching the last segment the way `rewrite_date_procedural_alias` does would hijack a
///    user's `App\var_export`, which resolves to `App\var_export` and is a different function.
/// 2. The prelude's internal helper must be declared. `inject_if_used` skips injection entirely
///    when the program declares its own global `var_export`, so a declared
///    `__elephc_var_export_str` means "the elephc prelude owns this name" and its absence means a
///    user definition (or no definition at all) must be left alone.
fn rewrite_var_export_return_flag(
    name: &str,
    args: &[Expr],
    symbols: &Symbols,
) -> Option<ExprKind> {
    let global = name.trim_start_matches('\\');
    if global.contains('\\')
        || !global.eq_ignore_ascii_case("var_export")
        || !symbols.declares_function(crate::var_export_prelude::RENDER_HELPER)
    {
        return None;
    }
    let (value, flag) = var_export_call_arguments(args)?;
    match flag.map(|flag| &flag.kind) {
        None | Some(ExprKind::BoolLiteral(false)) => Some(ExprKind::FunctionCall {
            name: resolved_name(crate::var_export_prelude::ECHO_HELPER.to_string()),
            args: vec![value.clone()],
        }),
        Some(ExprKind::BoolLiteral(true)) => Some(ExprKind::FunctionCall {
            name: resolved_name(crate::var_export_prelude::RENDER_HELPER.to_string()),
            args: vec![
                value.clone(),
                Expr::new(ExprKind::IntLiteral(0), value.span),
            ],
        }),
        Some(_) => None,
    }
}

/// Retargets a compiler-owned CLI `ini_set($option, $value)` call to its internal dispatch helper
/// and appends the call-site line used by php-src's invalid-`date.timezone` warning.
///
/// The global-name and helper-declaration guards ensure a user-defined or namespaced `ini_set`
/// remains untouched. The public wrapper retains the real two-parameter signature for reflection
/// and first-class callable use; only direct calls receive the hidden diagnostic line.
fn rewrite_cli_ini_set(
    name: &str,
    args: &[Expr],
    call_span: crate::span::Span,
    symbols: &Symbols,
) -> Option<ExprKind> {
    let global = name.trim_start_matches('\\');
    if global.contains('\\')
        || !global.eq_ignore_ascii_case("ini_set")
        || args.len() != 2
        || !symbols.declares_function(crate::opcache_prelude::CLI_INI_SET_DISPATCH_HELPER)
    {
        return None;
    }
    let mut call_args = args.to_vec();
    call_args.push(Expr::new(
        ExprKind::IntLiteral(call_span.line as i64),
        call_span,
    ));
    Some(ExprKind::FunctionCall {
        name: resolved_name(
            crate::opcache_prelude::CLI_INI_SET_DISPATCH_HELPER.to_string(),
        ),
        args: call_args,
    })
}

/// Applies PHP's weak scalar coercion to the `$datetime` argument of
/// `DateTime[Immutable]::createFromFormat()`, preserving named-argument wrappers and source spans.
fn coerce_create_from_format_datetime_arg(args: &mut [Expr]) {
    for (index, arg) in args.iter_mut().enumerate() {
        let value = match &arg.kind {
            ExprKind::NamedArg { name, value } if name.eq_ignore_ascii_case("datetime") => {
                Some((**value).clone())
            }
            ExprKind::NamedArg { .. } | ExprKind::Spread(_) => None,
            _ if index == 1 => Some(arg.clone()),
            _ => None,
        };
        let Some(value) = value else {
            continue;
        };
        let cast = Expr::new(
            ExprKind::Cast {
                target: crate::parser::ast::CastType::String,
                expr: Box::new(value),
            },
            arg.span,
        );
        if let ExprKind::NamedArg { name, .. } = &arg.kind {
            *arg = Expr::new(
                ExprKind::NamedArg {
                    name: name.clone(),
                    value: Box::new(cast),
                },
                arg.span,
            );
        } else {
            *arg = cast;
        }
    }
}

/// Rewrites a procedural date/time alias call into the equivalent OOP expression, or returns `None`
/// when the function name (matched case-insensitively on its last segment) or its arity does not
/// correspond to a known alias. This maps PHP's procedural date API onto elephc's OOP classes
/// before type checking, so `date_create($s)` becomes `new DateTime($s)`, `date_diff($a, $b)`
/// becomes `$a->diff($b)`, and so on.
fn rewrite_date_procedural_alias(
    name: &str,
    args: &[Expr],
    call_span: crate::span::Span,
) -> Option<ExprKind> {
    let bare = name
        .rsplit('\\')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let method = |obj: usize, m: &str, rest: &[usize]| ExprKind::MethodCall {
        object: Box::new(args[obj].clone()),
        method: m.to_string(),
        args: rest.iter().map(|&i| args[i].clone()).collect(),
    };
    let static_call = |class: &str, m: &str| ExprKind::StaticMethodCall {
        // Mirror the StaticMethodCall resolution arm, which this rewrite output bypasses (it is
        // final, not re-resolved): canonicalize the receiver to its fully-qualified form and key the
        // method name with `php_symbol_key` for PHP's case-insensitive method lookup. Without both,
        // the static-method lookup misses and reports "Undefined method".
        receiver: StaticReceiver::Named(resolved_name(class.to_string())),
        method: php_symbol_key(m),
        args: args.to_vec(),
    };
    match bare.as_str() {
        // Keep validation at runtime so literal and computed formats share PHP's `int|false`
        // behavior. The synthetic helper delegates valid one-character specifiers to `date()`.
        "idate" if args.len() == 1 || args.len() == 2 => {
            let mut wrapper_args = args.to_vec();
            if wrapper_args.len() == 1 {
                wrapper_args.push(Expr::new(ExprKind::Null, call_span));
            }
            wrapper_args.push(Expr::new(
                ExprKind::IntLiteral(call_span.line as i64),
                call_span,
            ));
            Some(ExprKind::StaticMethodCall {
                receiver: StaticReceiver::Named(resolved_name("DateTime".to_string())),
                method: php_symbol_key("__elephc_idate"),
                args: wrapper_args,
            })
        }
        // mktime()/gmmktime(): one to six arguments are accepted; omitted trailing ones default to the
        // corresponding component of the current local (mktime) or UTC (gmmktime) time. Desugar to the
        // internal `__elephc_mktime_raw`/`__elephc_gmmktime_raw` builtins (which keep the fixed 6-int
        // runtime ABI), filling each omitted slot with `intval(date("G"|"i"|"s"|"n"|"j"|"Y"))` (or
        // `gmdate` for gmmktime) so the runtime always receives six integers. Up to 6 args pass through
        // verbatim; more than 6 falls through to the arity diagnostic. The `int|false` failure path
        // PHP can return for out-of-range inputs is not modeled — elephc's runtime always yields a
        // normalized timestamp — so the return stays `int`.
        "mktime" | "gmmktime" if (1..=6).contains(&args.len()) => {
            let is_gm = bare == "gmmktime";
            let date_fn = if is_gm { "gmdate" } else { "date" };
            let specs = ["G", "i", "s", "n", "j", "Y"];
            let mut full: Vec<Expr> = Vec::with_capacity(6);
            for i in 0..6 {
                let span = crate::span::Span::dummy();
                let date_call = Expr::new(
                    ExprKind::FunctionCall {
                        name: resolved_name(date_fn.to_string()),
                        args: vec![Expr::new(
                            ExprKind::StringLiteral(specs[i].to_string()),
                            span,
                        )],
                    },
                    span,
                );
                let current_component = Expr::new(
                    ExprKind::FunctionCall {
                        name: resolved_name("intval".to_string()),
                        args: vec![date_call],
                    },
                    span,
                );
                full.push(match args.get(i) {
                    Some(argument) => Expr::new(
                        ExprKind::NullCoalesce {
                            value: Box::new(argument.clone()),
                            default: Box::new(current_component),
                        },
                        argument.span,
                    ),
                    None => current_component,
                });
            }
            let raw_name = if is_gm { "__elephc_gmmktime_raw" } else { "__elephc_mktime_raw" };
            Some(ExprKind::FunctionCall {
                name: resolved_name(raw_name.to_string()),
                args: full,
            })
        }
        "date_create" if args.len() <= 2 => Some(static_call("DateTime", "__elephc_date_create")),
        "date_create_immutable" if args.len() <= 2 => {
            Some(static_call("DateTimeImmutable", "__elephc_date_create"))
        }
        "date_create_from_format" if args.len() == 2 || args.len() == 3 => {
            let mut call_args = args.to_vec();
            coerce_create_from_format_datetime_arg(&mut call_args);
            Some(ExprKind::StaticMethodCall {
                receiver: StaticReceiver::Named(resolved_name("DateTime".to_string())),
                method: php_symbol_key("createFromFormat"),
                args: call_args,
            })
        }
        "date_create_immutable_from_format" if args.len() == 2 || args.len() == 3 => {
            let mut call_args = args.to_vec();
            coerce_create_from_format_datetime_arg(&mut call_args);
            Some(ExprKind::StaticMethodCall {
                receiver: StaticReceiver::Named(resolved_name("DateTimeImmutable".to_string())),
                method: php_symbol_key("createFromFormat"),
                args: call_args,
            })
        }
        "date_parse_from_format" if args.len() == 2 => {
            Some(static_call("DateTime", "__elephc_date_parse_from_format"))
        }
        "date_parse" if args.len() == 1 => Some(static_call("DateTime", "__elephc_date_parse")),
        "date_sun_info" if args.len() == 3 => {
            Some(static_call("DateTime", "__elephc_date_sun_info"))
        }
        "strptime" if args.len() == 2 => Some(static_call("DateTime", "__elephc_strptime")),
        "timezone_name_from_abbr" if (1..=3).contains(&args.len()) => {
            Some(static_call("DateTime", "__elephc_timezone_name_from_abbr"))
        }
        // ---- ext/calendar: pure Serial-Day-Number conversions desugar to DateTime helpers ----
        "cal_to_jd" if args.len() == 4 => Some(static_call("DateTime", "__elephc_cal_to_jd")),
        "cal_from_jd" if args.len() == 2 => Some(static_call("DateTime", "__elephc_cal_from_jd")),
        "cal_days_in_month" if args.len() == 3 => {
            Some(static_call("DateTime", "__elephc_cal_days_in_month"))
        }
        "cal_info" if args.len() <= 1 => Some(static_call("DateTime", "__elephc_cal_info")),
        "gregoriantojd" if args.len() == 3 => {
            Some(static_call("DateTime", "__elephc_gregoriantojd"))
        }
        "jdtogregorian" if args.len() == 1 => {
            Some(static_call("DateTime", "__elephc_jdtogregorian"))
        }
        "juliantojd" if args.len() == 3 => Some(static_call("DateTime", "__elephc_juliantojd")),
        "jdtojulian" if args.len() == 1 => Some(static_call("DateTime", "__elephc_jdtojulian")),
        "frenchtojd" if args.len() == 3 => Some(static_call("DateTime", "__elephc_frenchtojd")),
        "jdtofrench" if args.len() == 1 => Some(static_call("DateTime", "__elephc_jdtofrench")),
        "jewishtojd" if args.len() == 3 => Some(static_call("DateTime", "__elephc_jewishtojd")),
        "jdtojewish" if (1..=3).contains(&args.len()) => {
            Some(static_call("DateTime", "__elephc_jdtojewish"))
        }
        "jddayofweek" if (1..=2).contains(&args.len()) => {
            Some(static_call("DateTime", "__elephc_jddayofweek"))
        }
        "jdmonthname" if args.len() == 2 => Some(static_call("DateTime", "__elephc_jdmonthname")),
        "jdtounix" if args.len() == 1 => Some(static_call("DateTime", "__elephc_jdtounix")),
        // unixtojd($timestamp = time()) — an omitted argument means "now", which is distinct
        // from an explicit 0 (the epoch), so the current time is injected only when omitted.
        "unixtojd" if args.len() <= 1 => {
            let call_args = if args.is_empty() {
                vec![Expr::new(
                    ExprKind::FunctionCall {
                        name: resolved_name("time".to_string()),
                        args: Vec::new(),
                    },
                    crate::span::Span::dummy(),
                )]
            } else {
                vec![args[0].clone()]
            };
            Some(ExprKind::StaticMethodCall {
                receiver: StaticReceiver::Named(resolved_name("DateTime".to_string())),
                method: php_symbol_key("__elephc_unixtojd"),
                args: call_args,
            })
        }
        // easter_days()/easter_date() default the year to the current year when omitted.
        "easter_days" | "easter_date" if args.len() <= 2 => {
            let method = if bare == "easter_date" {
                "__elephc_easter_date"
            } else {
                "__elephc_easter_days"
            };
            let mut call_args = Vec::new();
            if args.is_empty() {
                // (int) date("Y") — the current calendar year.
                let dummy = crate::span::Span::dummy();
                let year_str = Expr::new(ExprKind::StringLiteral("Y".to_string()), dummy);
                let date_call = Expr::new(
                    ExprKind::FunctionCall {
                        name: resolved_name("date".to_string()),
                        args: vec![year_str],
                    },
                    dummy,
                );
                call_args.push(Expr::new(
                    ExprKind::FunctionCall {
                        name: resolved_name("intval".to_string()),
                        args: vec![date_call],
                    },
                    dummy,
                ));
            } else {
                call_args.extend(args.iter().cloned());
            }
            Some(ExprKind::StaticMethodCall {
                receiver: StaticReceiver::Named(resolved_name("DateTime".to_string())),
                method: php_symbol_key(method),
                args: call_args,
            })
        }
        // date_sunrise($ts, ...) / date_sunset($ts, ...) -> DateTime::__elephc_date_sunfunc, with a
        // leading $which flag (0 = sunrise, 1 = sunset) prepended to the user's arguments. The
        // synthetic method supplies ini defaults for any optional latitude/longitude/zenith/offset.
        "date_sunrise" | "date_sunset" if (1..=6).contains(&args.len()) => {
            let which = if bare == "date_sunset" { 1 } else { 0 };
            let span = args[0].span;
            let mut call_args = vec![
                Expr::new(ExprKind::IntLiteral(which), span),
                Expr::new(ExprKind::IntLiteral(call_span.line as i64), call_span),
            ];
            call_args.extend(args.iter().cloned());
            Some(ExprKind::StaticMethodCall {
                receiver: StaticReceiver::Named(resolved_name("DateTime".to_string())),
                method: php_symbol_key("__elephc_date_sunfunc"),
                args: call_args,
            })
        }
        "gettimeofday" if args.len() <= 1 => {
            Some(static_call("DateTime", "__elephc_gettimeofday"))
        }
        "date_get_last_errors" if args.is_empty() => {
            Some(static_call("DateTime", "getLastErrors"))
        }
        // strftime($fmt[, $ts]) / gmstrftime(...) -> DateTime::__elephc_strftime($fmt, $ts|time(), utc).
        // The timestamp defaults to time() and the local/UTC flag is appended, so the synthetic
        // method receives a fixed 3-argument shape.
        "strftime" | "gmstrftime" if args.len() == 1 || args.len() == 2 => {
            let utc = bare == "gmstrftime";
            let span = args[0].span;
            let ts_arg = if args.len() == 2 {
                args[1].clone()
            } else {
                Expr::new(
                    ExprKind::FunctionCall { name: resolved_name("time".to_string()), args: vec![] },
                    span,
                )
            };
            Some(ExprKind::StaticMethodCall {
                receiver: StaticReceiver::Named(resolved_name("DateTime".to_string())),
                method: php_symbol_key("__elephc_strftime"),
                args: vec![
                    args[0].clone(),
                    ts_arg,
                    Expr::new(ExprKind::BoolLiteral(utc), span),
                    Expr::new(ExprKind::IntLiteral(call_span.line as i64), call_span),
                ],
            })
        }
        "timezone_open" if args.len() == 1 => {
            let source_line = Expr::new(
                ExprKind::NamedArg {
                    name: "sourceLine".to_string(),
                    value: Box::new(Expr::new(
                        ExprKind::IntLiteral(call_span.line as i64),
                        call_span,
                    )),
                },
                call_span,
            );
            Some(ExprKind::StaticMethodCall {
                receiver: StaticReceiver::Named(resolved_name("DateTimeZone".to_string())),
                method: php_symbol_key("__elephc_timezone_open"),
                args: vec![args[0].clone(), source_line],
            })
        }
        // timezone_identifiers_list([$group[, $country]]) and the equivalent static
        // DateTimeZone::listIdentifiers (rewritten below) both desugar to the
        // injected free function __elephc_list_identifiers, which filters a baked
        // group/country table. A free function is used (not the synthetic method) so
        // its built array<string> return keeps its element type and in_array works.
        "timezone_identifiers_list" if args.len() <= 2 => {
            let mut wrapper_args = args.to_vec();
            if wrapper_args.is_empty() {
                wrapper_args.push(Expr::new(ExprKind::IntLiteral(2047), call_span));
            }
            if wrapper_args.len() == 1 {
                wrapper_args.push(Expr::new(
                    ExprKind::StringLiteral(String::new()),
                    call_span,
                ));
            }
            wrapper_args.push(Expr::new(
                ExprKind::StringLiteral("timezone_identifiers_list".to_string()),
                call_span,
            ));
            Some(ExprKind::FunctionCall {
                name: resolved_name("__elephc_list_identifiers".to_string()),
                args: wrapper_args,
            })
        }
        // Reports the IANA release the bundled timezone-introspection data was
        // baked from. The value is read at Rust compile time from the same
        // version.data that crates/elephc-tz/data/generate.php writes alongside
        // the transitions/location/abbreviations tables, so it stays in lockstep
        // with the data the compiler embeds instead of a hand-maintained literal.
        "timezone_version_get" if args.is_empty() => {
            Some(ExprKind::StringLiteral(
                include_str!("../../crates/elephc-tz/data/version.data").trim().to_string(),
            ))
        }
        "date_interval_create_from_date_string" if args.len() == 1 => {
            let mut wrapper_args = args.to_vec();
            wrapper_args.push(Expr::new(
                ExprKind::IntLiteral(call_span.line as i64),
                call_span,
            ));
            Some(ExprKind::StaticMethodCall {
                receiver: StaticReceiver::Named(resolved_name("DateInterval".to_string())),
                method: php_symbol_key("__elephc_create_from_date_string"),
                args: wrapper_args,
            })
        }
        "date_diff" if args.len() == 2 => Some(method(0, "diff", &[1])),
        "date_diff" if args.len() == 3 => Some(method(0, "diff", &[1, 2])),
        "date_format" if args.len() == 2 => Some(method(0, "format", &[1])),
        "date_add" if args.len() == 2 => Some(static_call("DateTime", "__elephc_date_add")),
        "date_sub" if args.len() == 2 => {
            let mut wrapper_args = args.to_vec();
            wrapper_args.push(Expr::new(
                ExprKind::IntLiteral(call_span.line as i64),
                call_span,
            ));
            Some(ExprKind::StaticMethodCall {
                receiver: StaticReceiver::Named(resolved_name("DateTime".to_string())),
                method: php_symbol_key("__elephc_date_sub"),
                args: wrapper_args,
            })
        }
        "date_modify" if args.len() == 2 => {
            let mut wrapper_args = args.to_vec();
            wrapper_args.push(Expr::new(
                ExprKind::IntLiteral(call_span.line as i64),
                call_span,
            ));
            Some(ExprKind::StaticMethodCall {
                receiver: StaticReceiver::Named(resolved_name("DateTime".to_string())),
                method: php_symbol_key("__elephc_date_modify"),
                args: wrapper_args,
            })
        }
        "date_timestamp_get" if args.len() == 1 => Some(method(0, "getTimestamp", &[])),
        "date_timestamp_set" if args.len() == 2 => {
            let mut wrapper_args = args.to_vec();
            wrapper_args.push(Expr::new(
                ExprKind::IntLiteral(call_span.line as i64),
                call_span,
            ));
            Some(ExprKind::StaticMethodCall {
                receiver: StaticReceiver::Named(resolved_name("DateTime".to_string())),
                method: php_symbol_key("__elephc_date_timestamp_set"),
                args: wrapper_args,
            })
        }
        "date_timezone_get" if args.len() == 1 => Some(method(0, "getTimezone", &[])),
        "date_timezone_set" if args.len() == 2 => Some(method(0, "setTimezone", &[1])),
        "date_offset_get" if args.len() == 1 => Some(method(0, "getOffset", &[])),
        "date_date_set" if args.len() == 4 => Some(method(0, "setDate", &[1, 2, 3])),
        "date_isodate_set" if args.len() == 4 => Some(method(0, "setISODate", &[1, 2, 3])),
        "date_isodate_set" if args.len() == 3 => Some(method(0, "setISODate", &[1, 2])),
        "date_time_set" if args.len() == 3 => Some(method(0, "setTime", &[1, 2])),
        "date_time_set" if args.len() == 4 => Some(method(0, "setTime", &[1, 2, 3])),
        "date_time_set" if args.len() == 5 => Some(method(0, "setTime", &[1, 2, 3, 4])),
        "date_interval_format" if args.len() == 2 => Some(method(0, "format", &[1])),
        "timezone_name_get" if args.len() == 1 => Some(method(0, "getName", &[])),
        _ => None,
    }
}

/// The lowercase names of PHP's procedural date/time aliases, in one enumerable place.
///
/// Mirrors the name set in `rewrite_date_procedural_alias` (the alias arms there minus their
/// arity guards). This is the single source of truth behind both
/// [`is_date_procedural_alias`] (the predicate) and [`date_procedural_alias_names`] (the
/// enumeration): the dynamic `function_exists($name)` lowering bakes the enumeration into the
/// binary while the literal `function_exists('name')` fold calls the predicate, so keeping one
/// list makes it structurally impossible for the two paths to disagree about these names.
pub(crate) const DATE_PROCEDURAL_ALIASES: &[&str] = &[
    "idate",
    "mktime",
    "gmmktime",
    "date_create",
    "date_create_immutable",
    "date_create_from_format",
    "date_create_immutable_from_format",
    "date_parse_from_format",
    "date_parse",
    "date_sun_info",
    "date_sunrise",
    "date_sunset",
    "strptime",
    "timezone_name_from_abbr",
    "cal_to_jd",
    "cal_from_jd",
    "cal_days_in_month",
    "cal_info",
    "gregoriantojd",
    "jdtogregorian",
    "juliantojd",
    "jdtojulian",
    "frenchtojd",
    "jdtofrench",
    "jewishtojd",
    "jdtojewish",
    "jddayofweek",
    "jdmonthname",
    "jdtounix",
    "unixtojd",
    "easter_days",
    "easter_date",
    "gettimeofday",
    "date_get_last_errors",
    "strftime",
    "gmstrftime",
    "timezone_open",
    "timezone_identifiers_list",
    "timezone_location_get",
    "timezone_transitions_get",
    "timezone_abbreviations_list",
    "timezone_version_get",
    "date_interval_create_from_date_string",
    "date_diff",
    "date_format",
    "date_add",
    "date_sub",
    "date_modify",
    "date_timestamp_get",
    "date_timestamp_set",
    "date_timezone_get",
    "date_timezone_set",
    "date_offset_get",
    "date_date_set",
    "date_isodate_set",
    "date_time_set",
    "date_interval_format",
    "timezone_name_get",
    "timezone_offset_get",
];

/// Reports whether `name` matches one of PHP's procedural date/time aliases, regardless of arity.
///
/// Consults [`DATE_PROCEDURAL_ALIASES`] so `function_exists()` and other introspection builtins
/// recognize the same procedural surface that the resolver rewrites. Comparison is
/// case-insensitive on the last namespace segment, matching the resolver's behavior.
pub(crate) fn is_date_procedural_alias(name: &str) -> bool {
    let bare = name
        .rsplit('\\')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    DATE_PROCEDURAL_ALIASES.contains(&bare.as_str())
}

/// Returns the inclusive `(min, max)` argument-count range that
/// `rewrite_date_procedural_alias` accepts for a desugared procedural date/time alias, or
/// `None` when `name` is not such an alias.
///
/// This MUST stay in lockstep with the arity guards in `rewrite_date_procedural_alias`: it lets
/// the type checker turn a wrong-arity alias call (which fails to desugar and would otherwise be
/// reported as "Undefined function") into a precise arity diagnostic, matching how real builtins
/// like `checkdate()` are diagnosed. The `timezone_location_get`/`timezone_transitions_get`/
/// `timezone_abbreviations_list` introspection names are intentionally excluded: they are real
/// injected prelude functions (not rewrite arms), so their arity is validated normally.
pub(crate) fn date_procedural_alias_arity(name: &str) -> Option<(usize, usize)> {
    let bare = name
        .rsplit('\\')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let range = match bare.as_str() {
        "date_get_last_errors" | "timezone_version_get" => (0, 0),
        "mktime" | "gmmktime" => (1, 6),
        "date_create" | "date_create_immutable" => (0, 2),
        "cal_info" | "unixtojd" | "gettimeofday" => (0, 1),
        "easter_days" | "easter_date" | "timezone_identifiers_list" => (0, 2),
        "date_parse" | "jdtogregorian" | "jdtojulian" | "jdtofrench" | "jdtounix"
        | "timezone_open" | "date_interval_create_from_date_string" | "date_timestamp_get"
        | "date_timezone_get" | "date_offset_get" | "timezone_name_get" => (1, 1),
        "idate" | "jddayofweek" | "strftime" | "gmstrftime" => (1, 2),
        "timezone_name_from_abbr" | "jdtojewish" => (1, 3),
        "date_sunrise" | "date_sunset" => (1, 6),
        "date_diff" => (2, 3),
        "date_parse_from_format" | "cal_from_jd" | "jdmonthname" | "strptime"
        | "date_format" | "date_add" | "date_sub" | "date_modify" | "date_timestamp_set"
        | "date_timezone_set" | "date_interval_format" | "timezone_offset_get" => (2, 2),
        "date_create_from_format" | "date_create_immutable_from_format" => (2, 3),
        "date_sun_info" | "cal_days_in_month" | "gregoriantojd" | "juliantojd" | "frenchtojd"
        | "jewishtojd" => (3, 3),
        "date_isodate_set" => (3, 4),
        "date_time_set" => (3, 5),
        "cal_to_jd" | "date_date_set" => (4, 4),
        _ => return None,
    };
    Some(range)
}
