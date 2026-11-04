//! Purpose:
//! Rewrites one partial-application call into the closure that php-src would synthesize.
//!
//! Called from:
//! - `crate::pfa::walk::Desugar::transform_expr`.
//!
//! Key details:
//! - Bound argument expressions are evaluated where the PFA expression appears, by passing
//!   them to an immediately-invoked closure whose parameters the returned closure captures.
//! - Placeholder parameters inherit the underlying callable's names, types, defaults, and
//!   reference modes whenever those are visible to this pass.
//! - A trailing `...` forwards every remaining argument through a variadic capture.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::Name;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver, Stmt, StmtKind, TypeExpr};
use crate::span::Span;

use super::signatures::{new_expression_class, receiver_class, CallableSig, ParamInfo, Signatures};
use super::{PLACEHOLDER_VAR, VARIADIC_PLACEHOLDER_VAR};

/// The callable being partially applied.
pub(super) enum Callee {
    /// `foo(...)`.
    Function(Name),
    /// `Foo::bar(...)`.
    StaticMethod { receiver: StaticReceiver, method: String },
    /// `$object->bar(...)`.
    Method { object: Expr, method: String },
    /// `$callable(...)` where the callee is an arbitrary expression.
    Expr(Expr),
    /// `$closure(...)` parsed as a plain variable callee.
    ClosureVar(String),
}

/// State the rewrite needs from the enclosing declaration.
pub(super) struct LowerContext<'a> {
    /// Declared signatures.
    pub signatures: &'a Signatures,
    /// Class whose body contains the call, when there is one.
    pub current_class: Option<&'a str>,
    /// Best-effort `$variable -> class` map for method-signature lookup.
    pub locals: &'a HashMap<String, String>,
}

/// Returns whether an expression is the parser's `?` marker.
fn is_placeholder(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::Variable(name) if name == PLACEHOLDER_VAR)
}

/// Returns whether the inner expression of a spread is the parser's `...` marker.
fn is_variadic_placeholder(inner: &Expr) -> bool {
    matches!(&inner.kind, ExprKind::Variable(name) if name == VARIADIC_PLACEHOLDER_VAR)
}

/// Returns whether a call argument list carries at least one partial-application marker.
pub(super) fn args_contain_marker(args: &[Expr]) -> bool {
    args.iter().any(|arg| match &arg.kind {
        ExprKind::NamedArg { value, .. } => is_placeholder(value),
        ExprKind::Spread(inner) => is_variadic_placeholder(inner),
        _ => is_placeholder(arg),
    })
}

/// Rewrites one marked call.
pub(super) fn lower(
    callee: Callee,
    args: Vec<Expr>,
    span: Span,
    ctx: &LowerContext<'_>,
) -> Result<Expr, CompileError> {
    let (resolved, is_static, label) = resolve(&callee, ctx);
    if matches!(&callee, Callee::Method { .. }) && resolved.is_none() {
        return Err(CompileError::new(
            span,
            "Cannot create Closure for a method whose signature is not declared (magic __call partial application is not supported yet)",
        ));
    }
    let mut bound_exprs: Vec<Expr> = Vec::new();
    let mut captures: Vec<String> = Vec::new();
    let mut outer_prelude: Vec<Stmt> = Vec::new();
    let call_target = CallTarget::from_callee(
        callee,
        span,
        &mut bound_exprs,
        &mut captures,
        &mut outer_prelude,
    );

    let mut params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)> = Vec::new();
    let mut used: HashSet<String> = HashSet::new();
    let mut body_args: Vec<Expr> = Vec::new();
    let mut body_indices: Vec<Option<usize>> = Vec::new();
    let mut cursor = 0usize;
    let mut given = 0usize;
    let mut saw_named = false;
    let mut saw_spread = false;
    let mut variadic: Option<String> = None;
    let mut variadic_type: Option<TypeExpr> = None;
    let mut variadic_by_ref = false;
    let mut has_variadic = false;
    let mut marker_count = 0usize;
    let arg_count = args.len();

    for (index, arg) in args.into_iter().enumerate() {
        match arg.kind {
            ExprKind::NamedArg { name, value } => {
                saw_named = true;
                if is_placeholder(&value) {
                    marker_count += 1;
                    let param = named_param(&resolved, &name).ok_or_else(|| {
                        CompileError::new(span, &format!("Unknown named parameter ${}", name))
                    })?;
                    push_param(&mut params, &mut used, &param, false, span)?;
                    body_indices.push(named_param_index(&resolved, &name));
                    body_args.push(Expr::new(
                        ExprKind::NamedArg {
                            name,
                            value: Box::new(variable(&param.name, span)),
                        },
                        span,
                    ));
                } else if is_variadic_placeholder(&value) {
                    return Err(CompileError::new(
                        span,
                        "The variadic placeholder cannot be a named argument",
                    ));
                } else {
                    let value_type = named_param(&resolved, &name)
                        .and_then(|param| param.type_expr);
                    let capture = bind(
                        &mut bound_exprs,
                        &mut captures,
                        &mut outer_prelude,
                        *value,
                        value_type,
                        span,
                    );
                    body_indices.push(named_param_index(&resolved, &name));
                    body_args.push(Expr::new(
                        ExprKind::NamedArg {
                            name,
                            value: Box::new(variable(&capture, span)),
                        },
                        span,
                    ));
                }
            }
            ExprKind::Spread(inner) if is_variadic_placeholder(&inner) => {
                marker_count += 1;
                if index + 1 != arg_count {
                    return Err(CompileError::new(
                        span,
                        "The variadic placeholder must be the last argument",
                    ));
                }
                has_variadic = true;
                if resolved.is_none() {
                    let name = unique_name("__elephc_pfa_args", &used);
                    used.insert(name.clone());
                    variadic_type = None;
                    variadic_by_ref = false;
                    body_indices.push(None);
                    body_args.push(Expr::new(
                        ExprKind::Spread(Box::new(variable(&name, span))),
                        span,
                    ));
                    variadic = Some(name);
                }
            }
            ExprKind::Spread(inner) => {
                if saw_spread || variadic.is_some() {
                    return Err(CompileError::new(
                        span,
                        "Cannot use argument unpacking after a variadic placeholder",
                    ));
                }
                if marker_count > 0 || saw_named {
                    return Err(CompileError::new(
                        span,
                        "Cannot use argument unpacking after a placeholder",
                    ));
                }
                saw_spread = true;
                let capture = bind(
                    &mut bound_exprs,
                    &mut captures,
                    &mut outer_prelude,
                    *inner,
                    None,
                    span,
                );
                body_indices.push(None);
                body_args.push(Expr::new(
                    ExprKind::Spread(Box::new(variable(&capture, span))),
                    span,
                ));
            }
            _ if is_placeholder(&arg) => {
                marker_count += 1;
                if saw_named {
                    return Err(CompileError::new(
                        span,
                        "Cannot use positional placeholder after named argument",
                    ));
                }
                if saw_spread {
                    return Err(CompileError::new(
                        span,
                        "Cannot use positional placeholder after argument unpacking",
                    ));
                }
                given += 1;
                let (param, generated) = positional_param(&resolved, cursor, &label, given, span)?;
                push_param(&mut params, &mut used, &param, generated, span)?;
                body_indices.push(Some(cursor));
                body_args.push(variable(&param.name, span));
                cursor += 1;
            }
            _ => {
                if saw_named {
                    return Err(CompileError::new(
                        span,
                        "Cannot use positional argument after named argument",
                    ));
                }
                if saw_spread {
                    return Err(CompileError::new(
                        span,
                        "Cannot use positional argument after argument unpacking",
                    ));
                }
                let value_type = value_param_type(&resolved, cursor);
                let capture = bind(
                    &mut bound_exprs,
                    &mut captures,
                    &mut outer_prelude,
                    arg,
                    value_type,
                    span,
                );
                body_indices.push(Some(cursor));
                body_args.push(variable(&capture, span));
                cursor += 1;
                given += 1;
            }
        }
    }

    if marker_count == 0 {
        return Err(CompileError::new(
            span,
            "Partial application requires at least one placeholder",
        ));
    }

    if !has_variadic {
        if let Some(sig) = &resolved {
            let covered: HashSet<usize> = body_indices.iter().flatten().copied().collect();
            let missing_required = sig
                .params
                .iter()
                .enumerate()
                .any(|(index, param)| param.default.is_none() && !covered.contains(&index));
            if missing_required {
                let required = sig
                    .params
                    .iter()
                    .filter(|param| param.default.is_none())
                    .count();
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Partial application of {}() expects at least {} arguments, {} given",
                        label,
                        required,
                        body_args.len()
                    ),
                ));
            }
        }
    }

    let known_variadic = has_variadic && resolved.is_some();
    if known_variadic {
        if let Some(sig) = &resolved {
            let (ordered, variadic_info) = rebuild_known_variadic(
                sig,
                &mut params,
                &mut used,
                body_args,
                body_indices.clone(),
                span,
            )?;
            body_args = ordered;
            if let Some((name, ty, by_ref)) = variadic_info {
                variadic = Some(name);
                variadic_type = ty;
                variadic_by_ref = by_ref;
            }
        }
    }

    if !has_variadic {
        // PHP user closures ignore extra positional arguments. A hidden variadic keeps the
        // generated closure callable with them without forwarding them to the underlying call.
        let name = unique_name("__elephc_pfa_ignored", &used);
        used.insert(name.clone());
        variadic = Some(name);
        variadic_type = None;
        variadic_by_ref = false;
    }

    let call_expr = if known_variadic {
        call_target.build(body_args, span)
    } else if variadic.is_some() {
        call_target.build_with_spread(body_args, body_indices, span)
    } else {
        call_target.build(body_args, span)
    };
    let return_type = resolved.as_ref().and_then(|sig| sig.return_type.clone());
    let by_ref_return = resolved.as_ref().is_some_and(|sig| sig.by_ref_return);
    let inner_captures = captures.clone();
    // A `void` callee cannot be the operand of `return`; the generated closure performs the
    // call and returns no value, exactly like the source function.
    let body_stmt = match &return_type {
        Some(TypeExpr::Void) => Stmt::new(StmtKind::ExprStmt(call_expr), span),
        _ => Stmt::new(StmtKind::Return(Some(call_expr)), span),
    };
    let body = vec![body_stmt];
    let inner = Expr::new(
        ExprKind::Closure {
            params,
            variadic,
            variadic_by_ref,
            variadic_type,
            return_type,
            body,
            is_arrow: false,
            is_static: is_static,
            by_ref_return,
            captures: inner_captures,
            capture_refs: Vec::new(),
        },
        span,
    );
    if bound_exprs.is_empty() {
        return Ok(inner);
    }
    let outer_params = (0..bound_exprs.len())
        .map(|index| (format!("__elephc_pfa_bound_{}", index), None, None, false))
        .collect();
    let mut outer_body = outer_prelude;
    outer_body.push(Stmt::new(StmtKind::Return(Some(inner)), span));
    let outer = Expr::new(
        ExprKind::Closure {
            params: outer_params,
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            body: outer_body,
            is_arrow: false,
            is_static: false,
            by_ref_return: false,
            captures: Vec::new(),
            capture_refs: Vec::new(),
        },
        span,
    );
    Ok(Expr::new(
        ExprKind::ExprCall {
            callee: Box::new(outer),
            args: bound_exprs,
        },
        span,
    ))
}

/// The callable shape rebuilt inside the generated closure.
enum CallTarget {
    Function(Name),
    StaticMethod {
        receiver: StaticReceiver,
        method: String,
    },
    Method {
        object: String,
        method: String,
    },
    Expr {
        callee: String,
    },
    ClosureVar {
        var: String,
    },
}

impl CallTarget {
    /// Consumes the callee, binding every value it must evaluate exactly once.
    fn from_callee(
        callee: Callee,
        span: Span,
        bound: &mut Vec<Expr>,
        captures: &mut Vec<String>,
        outer_prelude: &mut Vec<Stmt>,
    ) -> Self {
        match callee {
            Callee::Function(name) => Self::Function(name),
            Callee::StaticMethod { receiver, method } => Self::StaticMethod { receiver, method },
            Callee::Method { object, method } => {
                let capture = bind(bound, captures, outer_prelude, object, None, span);
                Self::Method {
                    object: capture,
                    method,
                }
            }
            Callee::Expr(callee) => {
                let capture = bind(bound, captures, outer_prelude, callee, None, span);
                Self::Expr { callee: capture }
            }
            Callee::ClosureVar(var) => {
                let capture = bind(
                    bound,
                    captures,
                    outer_prelude,
                    variable(&var, span),
                    None,
                    span,
                );
                Self::ClosureVar { var: capture }
            }
        }
    }

    /// Emits the underlying call with fixed arguments in parameter order.
    ///
    /// PHP forbids ordinary unpacking after a named argument, while the PFA `...` placeholder
    /// is written after them. Fixed arguments are therefore reordered by their underlying
    /// parameter index, the spread follows the contiguous prefix, and only unresolved named
    /// arguments stay named after it.
    fn build_with_spread(
        self,
        args: Vec<Expr>,
        indices: Vec<Option<usize>>,
        span: Span,
    ) -> Expr {
        let mut fixed: std::collections::BTreeMap<usize, Expr> = std::collections::BTreeMap::new();
        let mut spread = None;
        let mut trailing_named: Vec<(String, Expr)> = Vec::new();
        for (arg, index) in args.into_iter().zip(indices) {
            match arg.kind {
                ExprKind::Spread(_) => spread = Some(arg),
                ExprKind::NamedArg { name, value } => match index {
                    Some(index) => {
                        fixed.insert(index, *value);
                    }
                    None => trailing_named.push((name, *value)),
                },
                _ => match index {
                    Some(index) => {
                        fixed.insert(index, arg);
                    }
                    None => trailing_named.push((String::new(), arg)),
                },
            }
        }
        let mut ordered: Vec<Expr> = fixed.into_values().collect();
        if let Some(spread) = spread {
            ordered.push(spread);
        }
        for (name, value) in trailing_named {
            if name.is_empty() {
                ordered.push(value);
            } else {
                ordered.push(Expr::new(
                    ExprKind::NamedArg {
                        name,
                        value: Box::new(value),
                    },
                    span,
                ));
            }
        }
        self.build(ordered, span)
    }

    /// Emits the underlying call with the rebuilt argument list.
    fn build(self, args: Vec<Expr>, span: Span) -> Expr {
        match self {
            Self::Function(name) => Expr::new(ExprKind::FunctionCall { name, args }, span),
            Self::StaticMethod { receiver, method } => Expr::new(
                ExprKind::StaticMethodCall {
                    receiver,
                    method,
                    args,
                },
                span,
            ),
            Self::Method { object, method } => Expr::new(
                ExprKind::MethodCall {
                    object: Box::new(variable(&object, span)),
                    method,
                    args,
                },
                span,
            ),
            Self::Expr { callee } => Expr::new(
                ExprKind::ExprCall {
                    callee: Box::new(variable(&callee, span)),
                    args,
                },
                span,
            ),
            Self::ClosureVar { var } => Expr::new(ExprKind::ClosureCall { var, args }, span),
        }
    }
}

/// Resolves the callable signature and whether the generated closure is static.
fn resolve(
    callee: &Callee,
    ctx: &LowerContext<'_>,
) -> (Option<CallableSig>, bool, String) {
    match callee {
        Callee::Function(name) => {
            let sig = ctx.signatures.function(name);
            (
                sig,
                true,
                name.as_str().rsplit('\\').next().unwrap_or(name.as_str()).to_string(),
            )
        }
        Callee::StaticMethod { receiver, method } => {
            let class = receiver_class(receiver, ctx.current_class, ctx.signatures);
            let sig = class.as_deref().and_then(|class| {
                ctx.signatures.method(class, method, true)
            });
            (
                sig,
                true,
                format!(
                    "{}::{}",
                    class.unwrap_or_else(|| "static".to_string()),
                    method
                ),
            )
        }
        Callee::Method { object, method } => {
            let class = infer_object_class(object, ctx);
            let sig = class.as_deref().and_then(|class| {
                ctx.signatures
                    .method(class, method, false)
                    .or_else(|| ctx.signatures.method(class, method, true))
            });
            let is_static = sig.as_ref().is_some_and(|sig| sig.is_static);
            (sig, is_static, format!("->{}", method))
        }
        Callee::Expr(_) | Callee::ClosureVar(_) => (None, false, "callable".to_string()),
    }
}

/// Best-effort receiver type for a method call.
fn infer_object_class(object: &Expr, ctx: &LowerContext<'_>) -> Option<String> {
    match &object.kind {
        ExprKind::This => ctx.current_class.map(str::to_string),
        ExprKind::Variable(name) => ctx.locals.get(name).cloned(),
        _ => new_expression_class(object),
    }
}

/// Rebuilds a known-signature variadic partial application around its unbound parameters.
///
/// The `...` placeholder exposes every regular parameter that was not explicitly bound, in
/// declaration order. Extras flow through the underlying variadic parameter when it has one,
/// or through a hidden trailing variadic that PHP user functions ignore and `func_get_args()`
/// still sees.
fn rebuild_known_variadic(
    sig: &CallableSig,
    params: &mut Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    used: &mut HashSet<String>,
    body_args: Vec<Expr>,
    body_indices: Vec<Option<usize>>,
    span: Span,
) -> Result<(Vec<Expr>, Option<(String, Option<TypeExpr>, bool)>), CompileError> {
    let mut specified: std::collections::BTreeMap<usize, Expr> = std::collections::BTreeMap::new();
    let mut extra_named: Vec<Expr> = Vec::new();
    for (arg, index) in body_args.into_iter().zip(body_indices) {
        match index {
            Some(index) => {
                let value = match arg.kind {
                    ExprKind::NamedArg { value, .. } => *value,
                    _ => arg,
                };
                if specified.insert(index, value).is_some() {
                    return Err(CompileError::new(
                        span,
                        "Duplicate argument in partial application",
                    ));
                }
            }
            None => match arg.kind {
                ExprKind::NamedArg { .. } => extra_named.push(arg),
                ExprKind::Spread(_) => {}
                _ => extra_named.push(arg),
            },
        }
    }

    for index in 0..sig.params.len() {
        if specified.contains_key(&index) {
            continue;
        }
        let param = &sig.params[index];
        let name = unique_name(&param.name, used);
        if name != param.name {
            return Err(CompileError::new(
                span,
                &format!("Named parameter ${} overwrites previous placeholder", param.name),
            ));
        }
        used.insert(name.clone());
        params.push((
            name.clone(),
            param.type_expr.clone(),
            param.default.clone(),
            param.by_ref,
        ));
        specified.insert(index, variable(&name, span));
    }

    let mut ordered = Vec::with_capacity(specified.len() + 1);
    for index in 0..sig.params.len() {
        if let Some(value) = specified.remove(&index) {
            ordered.push(value);
        }
    }
    // Explicit placeholders that reached into the variadic portion keep their order.
    while let Some((_, value)) = specified.pop_first() {
        ordered.push(value);
    }

    let variadic = if let Some(param) = &sig.variadic {
        let name = unique_name(&param.name, used);
        if name != param.name {
            return Err(CompileError::new(
                span,
                &format!("Named parameter ${} overwrites previous placeholder", param.name),
            ));
        }
        used.insert(name.clone());
        ordered.push(Expr::new(
            ExprKind::Spread(Box::new(variable(&name, span))),
            span,
        ));
        Some((name, param.type_expr.clone(), param.by_ref))
    } else {
        let name = unique_name("__elephc_pfa_extra", used);
        used.insert(name.clone());
        ordered.push(Expr::new(
            ExprKind::Spread(Box::new(variable(&name, span))),
            span,
        ));
        Some((name, None, false))
    };
    ordered.extend(extra_named);
    Ok((ordered, variadic))
}

/// Resolves the parameter a positional placeholder occupies.
///
/// Returns `true` in the second slot when the name is generated for a variadic tail, where a
/// collision is resolved by skipping to the next numeric suffix instead of reporting an error.
fn positional_param(
    sig: &Option<CallableSig>,
    cursor: usize,
    label: &str,
    given: usize,
    span: Span,
) -> Result<(ParamInfo, bool), CompileError> {
    let Some(sig) = sig else {
        return Ok((
            ParamInfo {
                name: format!("pfa{}", cursor),
                type_expr: None,
                default: None,
                by_ref: false,
            },
            false,
        ));
    };
    if cursor < sig.params.len() {
        return Ok((sig.params[cursor].clone(), false));
    }
    if let Some(variadic) = &sig.variadic {
        return Ok((
            ParamInfo {
                name: format!("{}{}", variadic.name, cursor - sig.params.len()),
                type_expr: variadic.type_expr.clone(),
                default: None,
                by_ref: variadic.by_ref,
            },
            true,
        ));
    }
    Err(CompileError::new(
        span,
        &format!(
            "Partial application of {}() expects at most {} arguments, {} given",
            label,
            sig.params.len(),
            given
        ),
    ))
}

/// Resolves a named placeholder's parameter.
fn named_param(sig: &Option<CallableSig>, name: &str) -> Option<ParamInfo> {
    let Some(sig) = sig else {
        return Some(ParamInfo {
            name: name.to_string(),
            type_expr: None,
            default: None,
            by_ref: false,
        });
    };
    if let Some(param) = sig.params.iter().find(|param| param.name == name) {
        return Some(param.clone());
    }
    sig.variadic.as_ref().map(|variadic| ParamInfo {
        name: name.to_string(),
        type_expr: variadic.type_expr.clone(),
        default: None,
        by_ref: variadic.by_ref,
    })
}

/// Returns the underlying parameter index a named argument addresses.
fn named_param_index(sig: &Option<CallableSig>, name: &str) -> Option<usize> {
    sig.as_ref()?
        .params
        .iter()
        .position(|param| param.name == name)
}

/// Appends one closure parameter, rejecting a duplicate PHP parameter name.
fn push_param(
    params: &mut Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    used: &mut HashSet<String>,
    param: &ParamInfo,
    generated: bool,
    span: Span,
) -> Result<(), CompileError> {
    let name = if generated {
        unique_name(&param.name, used)
    } else {
        param.name.clone()
    };
    if used.contains(&name) {
        return Err(CompileError::new(
            span,
            &format!("Named parameter ${} overwrites previous placeholder", param.name),
        ));
    }
    used.insert(name.clone());
    params.push((
        name,
        param.type_expr.clone(),
        param.default.clone(),
        param.by_ref,
    ));
    Ok(())
}

/// Returns `preferred` unless it is already taken, in which case a numeric suffix is added.
fn unique_name(preferred: &str, used: &HashSet<String>) -> String {
    if !used.contains(preferred) {
        return preferred.to_string();
    }
    unique_suffix_name(preferred, used)
}

/// Returns a free `{preferred}{n}` name.
fn unique_suffix_name(preferred: &str, used: &HashSet<String>) -> String {
    for suffix in 0.. {
        let candidate = format!("{}{}", preferred, suffix);
        if !used.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!("unbounded numeric suffixes")
}

/// Evaluates `value` at closure-creation time and exposes it as a capture.
///
/// A known parameter type is applied by an immediately-invoked typed closure, so a mismatched
/// bound value raises PHP's catchable `TypeError` at PFA creation rather than being rejected by
/// the static checker.
fn bind(
    bound: &mut Vec<Expr>,
    captures: &mut Vec<String>,
    outer_prelude: &mut Vec<Stmt>,
    value: Expr,
    ty: Option<TypeExpr>,
    span: Span,
) -> String {
    let index = bound.len();
    let bound_name = format!("__elephc_pfa_bound_{}", index);
    bound.push(value);
    let Some(ty) = ty.filter(|ty| !is_mixed_type(ty)) else {
        captures.push(bound_name.clone());
        return bound_name;
    };
    let capture = format!("__elephc_pfa_typed_{}", index);
    outer_prelude.push(Stmt::new(
        StmtKind::Assign {
            name: capture.clone(),
            value: Expr::new(
                ExprKind::ExprCall {
                    callee: Box::new(typed_identity(&ty, span)),
                    args: vec![variable(&bound_name, span)],
                },
                span,
            ),
        },
        span,
    ));
    captures.push(capture.clone());
    capture
}

/// Builds `(function (T $value): T { return $value; })`.
fn typed_identity(ty: &TypeExpr, span: Span) -> Expr {
    Expr::new(
        ExprKind::Closure {
            params: vec![("__elephc_pfa_value".to_string(), Some(ty.clone()), None, false)],
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: Some(ty.clone()),
            body: vec![Stmt::new(
                StmtKind::Return(Some(variable("__elephc_pfa_value", span))),
                span,
            )],
            is_arrow: false,
            is_static: true,
            by_ref_return: false,
            captures: Vec::new(),
            capture_refs: Vec::new(),
        },
        span,
    )
}

/// Returns whether a type expression is the unconstraining `mixed` hint.
fn is_mixed_type(ty: &TypeExpr) -> bool {
    matches!(ty, TypeExpr::Named(name) if name.as_str().trim_start_matches('\\').eq_ignore_ascii_case("mixed"))
}

/// Returns the declared type of the positional argument at `cursor`, when known.
fn value_param_type(sig: &Option<CallableSig>, cursor: usize) -> Option<TypeExpr> {
    let sig = sig.as_ref()?;
    if let Some(param) = sig.params.get(cursor) {
        return param.type_expr.clone();
    }
    sig.variadic
        .as_ref()
        .and_then(|param| param.type_expr.clone())
}

/// Builds a local-variable expression.
fn variable(name: &str, span: Span) -> Expr {
    Expr::new(ExprKind::Variable(name.to_string()), span)
}
