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
            // Procedural date/time aliases desugar to the equivalent OOP construction or method
            // call (e.g. date_create($s) -> new DateTime($s), date_diff($a, $b) -> $a->diff($b)).
            // Skip the rewrite when the resolved name is a user-declared function, so a
            // user-defined (e.g. namespaced `App\date_diff`) call is never hijacked.
            if symbols.declares_function(&function_name) {
                ExprKind::FunctionCall {
                    name: resolved_name(function_name),
                    args: resolved_args,
                }
            } else if let Some(rewritten) =
                rewrite_date_procedural_alias(&function_name, &resolved_args)
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
            variadic_type,
            return_type,
            body,
            is_arrow,
            is_static,
            captures,
            capture_refs,
        } => ExprKind::Closure {
            params: resolve_params(params, current_namespace, imports, symbols),
            variadic: variadic.clone(),
            variadic_type: variadic_type.clone(),
            return_type: return_type
                .as_ref()
                .map(|ty| resolve_type_expr(ty, current_namespace, imports, symbols)),
            body: resolve_stmt_list(body, current_namespace, imports, symbols)
                .expect("name resolver bug: closure body resolution failed"),
            is_arrow: *is_arrow,
            is_static: *is_static,
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
        ExprKind::ConstRef(name) => ExprKind::ConstRef(resolved_name(resolve_constant_name(
            name,
            current_namespace,
            imports,
            symbols,
        ))),
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name: resolved_name(resolve_special_or_class_name(
                class_name,
                current_namespace,
                imports,
                symbols,
            )),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
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
        ExprKind::ScopedConstantAccess { receiver, name } => ExprKind::ScopedConstantAccess {
            receiver: match receiver {
                StaticReceiver::Named(name) => StaticReceiver::Named(resolved_name(
                    resolve_special_or_class_name(name, current_namespace, imports, symbols),
                )),
                _ => receiver.clone(),
            },
            name: name.clone(),
        },
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
            method: method.clone(),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        ExprKind::NullsafeMethodCall { object, method, args } => ExprKind::NullsafeMethodCall {
            object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
            method: method.clone(),
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
            let resolved_method = php_symbol_key(method);
            let resolved_args: Vec<Expr> = args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect();
            // DateTimeZone::listIdentifiers([$group[, $country]]) desugars to the
            // injected __elephc_list_identifiers free function (mirrors the
            // timezone_identifiers_list arm in rewrite_date_procedural_alias): a
            // function's flow-inferred array<string> return keeps its element type,
            // so in_array works on the filtered result, where the synthetic method
            // would yield scalar mixed and regress in_array.
            if resolved_method == "listidentifiers"
                && resolved_args.len() <= 2
                && matches!(
                    &resolved_receiver,
                    StaticReceiver::Named(name)
                        if name.last_segment().is_some_and(|seg| seg.eq_ignore_ascii_case("DateTimeZone"))
                            && !symbols.declares_class_like(&name.as_canonical())
                )
            {
                ExprKind::FunctionCall {
                    name: resolved_name("__elephc_list_identifiers".to_string()),
                    args: resolved_args,
                }
            } else {
                ExprKind::StaticMethodCall {
                    receiver: resolved_receiver,
                    method: resolved_method,
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
                method: php_symbol_key(method),
            },
            CallableTarget::Method { object, method } => CallableTarget::Method {
                object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
                method: php_symbol_key(method),
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

/// Rewrites a procedural date/time alias call into the equivalent OOP expression, or returns `None`
/// when the function name (matched case-insensitively on its last segment) or its arity does not
/// correspond to a known alias. This maps PHP's procedural date API onto elephc's OOP classes
/// before type checking, so `date_create($s)` becomes `new DateTime($s)`, `date_diff($a, $b)`
/// becomes `$a->diff($b)`, and so on.
fn rewrite_date_procedural_alias(name: &str, args: &[Expr]) -> Option<ExprKind> {
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
    let new_object = |class: &str| ExprKind::NewObject {
        class_name: crate::names::Name::unqualified(class),
        args: args.to_vec(),
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
        // idate($fmt[, $ts]) returns the integer value of a single date() specifier; it is exactly
        // `intval(date($fmt[, $ts]))` for every integer-yielding specifier, so desugar to that and
        // reuse the existing date()/intval() codegen rather than a dedicated builtin.
        "idate" if args.len() == 1 || args.len() == 2 => {
            let date_call = Expr::new(
                ExprKind::FunctionCall {
                    name: resolved_name("date".to_string()),
                    args: args.to_vec(),
                },
                args[0].span,
            );
            Some(ExprKind::FunctionCall {
                name: resolved_name("intval".to_string()),
                args: vec![date_call],
            })
        }
        // mktime()/gmmktime() (PHP 8.0+): every argument is optional; omitted ones default to the
        // corresponding component of the current local (mktime) or UTC (gmmktime) time. Desugar to the
        // internal `__elephc_mktime_raw`/`__elephc_gmmktime_raw` builtins (which keep the fixed 6-int
        // runtime ABI), filling each omitted slot with `intval(date("G"|"i"|"s"|"n"|"j"|"Y"))` (or
        // `gmdate` for gmmktime) so the runtime always receives six integers. Up to 6 args pass through
        // verbatim; more than 6 falls through to the arity diagnostic. The `int|false` failure path
        // PHP can return for out-of-range inputs is not modeled — elephc's runtime always yields a
        // normalized timestamp — so the return stays `int`.
        "mktime" | "gmmktime" if args.len() <= 6 => {
            let is_gm = bare == "gmmktime";
            let date_fn = if is_gm { "gmdate" } else { "date" };
            let specs = ["G", "i", "s", "n", "j", "Y"];
            let mut full: Vec<Expr> = Vec::with_capacity(6);
            for i in 0..6 {
                full.push(match args.get(i) {
                    Some(a) => a.clone(),
                    None => {
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
                        Expr::new(
                            ExprKind::FunctionCall {
                                name: resolved_name("intval".to_string()),
                                args: vec![date_call],
                            },
                            span,
                        )
                    }
                });
            }
            let raw_name = if is_gm { "__elephc_gmmktime_raw" } else { "__elephc_mktime_raw" };
            Some(ExprKind::FunctionCall {
                name: resolved_name(raw_name.to_string()),
                args: full,
            })
        }
        "date_create" if args.len() <= 2 => Some(new_object("DateTime")),
        "date_create_immutable" if args.len() <= 2 => Some(new_object("DateTimeImmutable")),
        "date_create_from_format" if args.len() == 2 || args.len() == 3 => {
            Some(static_call("DateTime", "createFromFormat"))
        }
        "date_create_immutable_from_format" if args.len() == 2 || args.len() == 3 => {
            Some(static_call("DateTimeImmutable", "createFromFormat"))
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
            let mut call_args = vec![Expr::new(ExprKind::IntLiteral(which), span)];
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
                ],
            })
        }
        "timezone_open" if args.len() == 1 => Some(new_object("DateTimeZone")),
        // timezone_identifiers_list([$group[, $country]]) and the equivalent static
        // DateTimeZone::listIdentifiers (rewritten below) both desugar to the
        // injected free function __elephc_list_identifiers, which filters a baked
        // group/country table. A free function is used (not the synthetic method) so
        // its built array<string> return keeps its element type and in_array works.
        "timezone_identifiers_list" if args.len() <= 2 => Some(ExprKind::FunctionCall {
            name: resolved_name("__elephc_list_identifiers".to_string()),
            args: args.to_vec(),
        }),
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
            Some(static_call("DateInterval", "createFromDateString"))
        }
        "date_diff" if args.len() == 2 => Some(method(0, "diff", &[1])),
        "date_diff" if args.len() == 3 => Some(method(0, "diff", &[1, 2])),
        "date_format" if args.len() == 2 => Some(method(0, "format", &[1])),
        "date_add" if args.len() == 2 => Some(method(0, "add", &[1])),
        "date_sub" if args.len() == 2 => Some(method(0, "sub", &[1])),
        "date_modify" if args.len() == 2 => Some(method(0, "modify", &[1])),
        "date_timestamp_get" if args.len() == 1 => Some(method(0, "getTimestamp", &[])),
        "date_timestamp_set" if args.len() == 2 => Some(method(0, "setTimestamp", &[1])),
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
        "timezone_offset_get" if args.len() == 2 => Some(method(0, "getOffset", &[1])),
        _ => None,
    }
}

/// Reports whether `name` matches one of PHP's procedural date/time aliases, regardless of arity.
///
/// Mirrors the name set in `rewrite_date_procedural_alias` (the alias arms there minus their
/// arity guards) so `function_exists()` and other introspection builtins can recognize the
/// same procedural surface that the resolver rewrites. Comparison is case-insensitive on the
/// last namespace segment, matching the resolver's behavior.
pub(crate) fn is_date_procedural_alias(name: &str) -> bool {
    let bare = name
        .rsplit('\\')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        bare.as_str(),
        "idate"
            | "mktime"
            | "gmmktime"
            | "date_create"
            | "date_create_immutable"
            | "date_create_from_format"
            | "date_create_immutable_from_format"
            | "date_parse_from_format"
            | "date_parse"
            | "date_sun_info"
            | "date_sunrise"
            | "date_sunset"
            | "strptime"
            | "timezone_name_from_abbr"
            | "cal_to_jd"
            | "cal_from_jd"
            | "cal_days_in_month"
            | "cal_info"
            | "gregoriantojd"
            | "jdtogregorian"
            | "juliantojd"
            | "jdtojulian"
            | "frenchtojd"
            | "jdtofrench"
            | "jewishtojd"
            | "jdtojewish"
            | "jddayofweek"
            | "jdmonthname"
            | "jdtounix"
            | "unixtojd"
            | "easter_days"
            | "easter_date"
            | "gettimeofday"
            | "date_get_last_errors"
            | "strftime"
            | "gmstrftime"
            | "timezone_open"
            | "timezone_identifiers_list"
            | "timezone_location_get"
            | "timezone_transitions_get"
            | "timezone_abbreviations_list"
            | "timezone_version_get"
            | "date_interval_create_from_date_string"
            | "date_diff"
            | "date_format"
            | "date_add"
            | "date_sub"
            | "date_modify"
            | "date_timestamp_get"
            | "date_timestamp_set"
            | "date_timezone_get"
            | "date_timezone_set"
            | "date_offset_get"
            | "date_date_set"
            | "date_isodate_set"
            | "date_time_set"
            | "date_interval_format"
            | "timezone_name_get"
            | "timezone_offset_get"
    )
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
        "mktime" | "gmmktime" => (0, 6),
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
