//! Purpose:
//! Shared checker hook and EIR lowering for the nine `xml_set_*_handler()` registry
//! builtins: an unannotated handler closure gets its parameters typed from the event it
//! will receive, then the call lowers to the xml prelude's `__elephc_xml_set_*` twin.
//!
//! Called from:
//! - The `xml_set_*_handler` homes in this directory.
//!
//! Key details:
//! - WHY THESE ARE REGISTRY BUILTINS. A closure literal passed to a `mixed` parameter of an
//!   ordinary (prelude) function keeps elephc's unhinted placeholder types for its
//!   unannotated parameters, so `function ($parser, $name, $attributes)` — the way every PHP
//!   SAX program is written — would receive its arguments mistyped. The checker hook types
//!   them exactly as php-src calls the handler (`XMLParser`, `string`, `array`, ...).
//! - A closure literal's shape is validated against the event: a variadic parameter and
//!   more required parameters than the event supplies are compile errors (the run time
//!   would hand the variadic pack garbage, respectively abort with an uncatchable fatal),
//!   and a bare `array` annotation at an attribute slot is applied AS IF unannotated —
//!   `handler_closure_params` — so the closure takes the event's `array<string,string>`
//!   instead of a boxed `array` the dynamic invoker cannot deliver. The lowering
//!   (`ir_lower::expr::builtin_special_args::lower_xml_handler_setter_args`) uses the same
//!   helper, so checker and codegen see one parameter list.
//! - Non-closure handlers (function names, `[$object, 'method']`, method-name strings for
//!   `xml_set_object()`, null) are inferred normally and validated by the prelude at run time.
//!   A function-name literal must be spelled exactly as declared: the run-time dynamic
//!   callable lookup is case-sensitive, so a case-insensitive compile-time match would
//!   only defer the failure to an unrelated `ValueError` at run time.
//! - The lowering is one `Op::Call` to the prelude twin with the normalized operands; the
//!   reachability scan keeps the twin alive (see `super::prelude_helpers_for`).

use crate::builtins::semantics::{
    BuiltinArgumentLowering, BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering,
    BuiltinLoweringContext, BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership,
    BuiltinResultType, BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::ir::Effects;
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{Expr, ExprKind, TypeExpr};
use crate::types::call_args::regular_param_count;
use crate::types::checker::builtins::callback_dummy_arg_for_type;
use crate::types::{FunctionSig, PhpType, TypeEnv};

/// One closure parameter as the parser declares it: name, annotation, default, by-ref.
pub(crate) type ClosureParam = (String, Option<TypeExpr>, Option<Expr>, bool);

/// The static type of one handler parameter, as php-src passes it.
#[derive(Clone, Copy)]
pub(crate) enum HandlerParam {
    /// The `XMLParser` object every handler receives first.
    Parser,
    /// A string argument (names, data, ids that are always present).
    Str,
    /// The start-element attribute map.
    Attributes,
    /// A value that may be a string or `false` (optional ids, PI data, the default prefix).
    Mixed,
}

impl HandlerParam {
    /// Returns the checker type used as the closure parameter hint.
    fn php_type(self) -> PhpType {
        match self {
            Self::Parser => PhpType::Object("XMLParser".to_string()),
            Self::Str => PhpType::Str,
            Self::Attributes => PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Str),
            },
            Self::Mixed => PhpType::Mixed,
        }
    }
}

/// The handler shapes one setter installs, in parameter order after `$parser`.
pub(crate) struct SetterSpec {
    /// The prelude twin the call lowers to.
    pub(super) helper: &'static str,
    /// Handler parameter hints per handler argument (the setter's parameters after `$parser`).
    pub(super) handlers: &'static [&'static [HandlerParam]],
}

impl SetterSpec {
    /// Returns the checker/lowering parameter hints of every handler argument, in order.
    pub(crate) fn param_hints(&self) -> Vec<Vec<PhpType>> {
        self.handlers
            .iter()
            .map(|handler| handler.iter().map(|param| param.php_type()).collect())
            .collect()
    }
}

/// `(XMLParser $parser, string $name, array $attributes)`.
pub(super) const START_ELEMENT: &[HandlerParam] =
    &[HandlerParam::Parser, HandlerParam::Str, HandlerParam::Attributes];
/// `(XMLParser $parser, string $data)` — end element, character data, default, end namespace.
pub(super) const PARSER_AND_STRING: &[HandlerParam] = &[HandlerParam::Parser, HandlerParam::Str];
/// `(XMLParser $parser, string $target, string|false $data)`.
pub(super) const PROCESSING_INSTRUCTION: &[HandlerParam] =
    &[HandlerParam::Parser, HandlerParam::Str, HandlerParam::Mixed];
/// `(XMLParser $parser, string $name, false $base, string $system_id, string|false $public_id, string $notation)`.
pub(super) const UNPARSED_ENTITY_DECL: &[HandlerParam] = &[
    HandlerParam::Parser,
    HandlerParam::Str,
    HandlerParam::Mixed,
    HandlerParam::Str,
    HandlerParam::Mixed,
    HandlerParam::Str,
];
/// `(XMLParser $parser, string $name, false $base, string|false $system_id, string|false $public_id)`.
pub(super) const NOTATION_DECL: &[HandlerParam] = &[
    HandlerParam::Parser,
    HandlerParam::Str,
    HandlerParam::Mixed,
    HandlerParam::Mixed,
    HandlerParam::Mixed,
];
/// `(XMLParser $parser, string $names, string $base, string $system_id, string|false $public_id)`.
pub(super) const EXTERNAL_ENTITY_REF: &[HandlerParam] = &[
    HandlerParam::Parser,
    HandlerParam::Str,
    HandlerParam::Str,
    HandlerParam::Str,
    HandlerParam::Mixed,
];
/// `(XMLParser $parser, string|false $prefix, string $uri)`.
pub(super) const START_NAMESPACE_DECL: &[HandlerParam] =
    &[HandlerParam::Parser, HandlerParam::Mixed, HandlerParam::Str];

/// Builds the shared semantic descriptor of a setter builtin.
pub(super) const fn setter_semantics(lower: crate::builtins::semantics::BuiltinLowerFn) -> BuiltinSemantics {
    BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        // Installing a handler rebinds parser state and may throw PHP's `ValueError`.
        effects: BuiltinEffects::Static(
            Effects::READS_HEAP
                .union(Effects::WRITES_HEAP)
                .union(Effects::ALLOC_HEAP)
                .union(Effects::REFCOUNT_OP)
                .union(Effects::MAY_THROW)
                .union(Effects::MAY_DEOPT),
        ),
        result_ownership: BuiltinResultOwnership::NonHeap,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::XmlHandlerSetter,
        callable: BuiltinCallablePolicy::StaticOnly(
            "handler setters type their closure arguments at the call site",
        ),
        lowering: BuiltinLowering::Eir(lower),
    }
}

/// Returns the expression behind a possibly named argument.
fn argument_value(arg: &Expr) -> &Expr {
    match &arg.kind {
        ExprKind::NamedArg { value, .. } => value,
        _ => arg,
    }
}

/// Returns the setter parameter index an argument binds (0 = `$parser`).
fn parameter_index(arg: &Expr, index: usize, parameters: &[&str]) -> Option<usize> {
    match &arg.kind {
        ExprKind::NamedArg { name, .. } => {
            let key = php_symbol_key(name);
            parameters.iter().position(|candidate| *candidate == key)
        }
        _ => (index < parameters.len()).then_some(index),
    }
}

/// Returns whether a hint is one of the array types an event supplies (the attribute map).
fn hint_is_array(hint: &PhpType) -> bool {
    matches!(hint, PhpType::Array(_) | PhpType::AssocArray { .. })
}

/// Returns whether an annotation is PHP's bare `array` (no key/value types).
fn is_bare_array_annotation(type_expr: &TypeExpr) -> bool {
    matches!(type_expr, TypeExpr::Named(name) if name.as_str().eq_ignore_ascii_case("array"))
}

/// Returns the handler closure's parameters as checker and lowering must both see them: a
/// bare `array` annotation at a slot whose event hint is an array type is dropped, so the
/// hint applies exactly as for an unannotated parameter.
///
/// WHY. A declared `array` parameter makes the closure a typed callee, and the dynamic
/// invoker the prelude calls it through boxes the attribute map into a shape that callee
/// cannot read (a pre-existing invoker limitation with associative arrays); the bare
/// annotation adds no information the hint does not already carry, so applying the hint
/// is both the safe and the precise choice. Every other annotation is kept verbatim.
pub(crate) fn handler_closure_params(params: &[ClosureParam], hints: &[PhpType]) -> Vec<ClosureParam> {
    params
        .iter()
        .enumerate()
        .map(|(index, (name, type_expr, default, by_ref))| {
            let drop_annotation = hints.get(index).is_some_and(hint_is_array)
                && type_expr.as_ref().is_some_and(is_bare_array_annotation);
            let type_expr = if drop_annotation { None } else { type_expr.clone() };
            (name.clone(), type_expr, default.clone(), *by_ref)
        })
        .collect()
}

/// Rejects the closure shapes the event cannot serve: a variadic parameter, and more
/// required parameters than the event passes (PHP throws `ArgumentCountError` from inside
/// the parse; the AOT run time has no such boundary and would abort).
fn check_handler_closure_shape(
    cx: &BuiltinCheckCtx,
    handler_parameter: &str,
    params: &[ClosureParam],
    variadic: &Option<String>,
    hints: &[PhpType],
    span: crate::span::Span,
) -> Result<(), CompileError> {
    if variadic.is_some() {
        return Err(CompileError::new(
            span,
            &format!(
                "{}(): handler closures cannot declare a variadic parameter: declare the parameters the event supplies",
                cx.name
            ),
        ));
    }
    let required = params
        .iter()
        .filter(|(_, _, default, _)| default.is_none())
        .count();
    if required > hints.len() {
        return Err(CompileError::new(
            span,
            &format!(
                "{}(): {} declares {required} required parameters but the {} event supplies {}",
                cx.name,
                handler_parameter.replace('_', " "),
                event_label(cx.name),
                hints.len()
            ),
        ));
    }
    Ok(())
}

/// Returns the event a setter installs handlers for, from its name:
/// `xml_set_character_data_handler` → `character data`.
fn event_label(setter: &str) -> String {
    setter
        .trim_start_matches("xml_set_")
        .trim_end_matches("_handler")
        .replace('_', " ")
}

/// Validates the parser argument and infers every handler argument, typing an unannotated
/// closure's parameters from `spec` for the slot it fills.
pub(super) fn check_setter(
    cx: &mut BuiltinCheckCtx,
    parameters: &[&str],
    spec: &SetterSpec,
) -> Result<PhpType, CompileError> {
    for (index, arg) in cx.args.iter().enumerate() {
        let value = argument_value(arg);
        let Some(position) = parameter_index(arg, index, parameters) else {
            cx.checker.infer_type(value, cx.env)?;
            continue;
        };
        if position == 0 {
            let ty = cx.checker.infer_type(value, cx.env)?;
            let can_be_parser = match ty.codegen_repr() {
                PhpType::Object(ref name) => php_symbol_key(name) == "xmlparser",
                PhpType::Mixed => true,
                _ => false,
            };
            // A declared-array extract is boxed. The prelude validates its runtime
            // class before narrowing the parser and reading any object storage.
            if !can_be_parser {
                return Err(CompileError::new(
                    value.span,
                    &format!(
                        "{}(): Argument #1 ($parser) must be of type XMLParser, {ty:?} given",
                        cx.name
                    ),
                ));
            }
            continue;
        }
        let hints = spec.param_hints().into_iter().nth(position - 1).unwrap_or_default();
        if let ExprKind::Closure {
            params,
            variadic,
            variadic_by_ref,
            return_type,
            body,
            captures,
            capture_refs,
            by_ref_return,
            ..
        } = &value.kind
        {
            check_handler_closure_shape(
                cx,
                parameters[position],
                params,
                variadic,
                &hints,
                value.span,
            )?;
            let params = handler_closure_params(params, &hints);
            cx.checker.infer_closure_type_with_param_hints(
                &params,
                variadic,
                *variadic_by_ref,
                return_type,
                body,
                captures,
                capture_refs,
                value,
                cx.env,
                &hints,
            )?;
            cx.checker.resolve_closure_signature_with_param_hints(
                &params,
                variadic,
                *variadic_by_ref,
                return_type,
                body,
                captures,
                *by_ref_return,
                value.span,
                cx.env,
                &hints,
            )?;
        } else {
            cx.checker.infer_type(value, cx.env)?;
            specialize_named_handler(cx, value, &hints)?;
        }
    }
    Ok(PhpType::Bool)
}

/// Gives a handler named statically — a function-name literal, a first-class callable, or a
/// literal `[$object, 'method']` pair — the call site it never has: its unannotated
/// parameters are specialized from the event types exactly as a direct call would type
/// them. Anything resolved at run time (`xml_set_object()` method names, names held in
/// variables) is left to the prelude's run-time binding.
///
/// PHP lets a callback declare fewer parameters than the event supplies, so the synthetic
/// call passes only as many arguments as the callee declares.
fn specialize_named_handler(
    cx: &mut BuiltinCheckCtx,
    value: &Expr,
    hints: &[PhpType],
) -> Result<(), CompileError> {
    if hints.is_empty() {
        return Ok(());
    }
    let span = value.span;
    let mut env = cx.env.clone();
    let dummy_args = |count: usize, env: &mut TypeEnv| -> Vec<Expr> {
        hints
            .iter()
            .take(count)
            .enumerate()
            .map(|(index, ty)| callback_dummy_arg_for_type(ty, index, span, env))
            .collect()
    };
    let call = match &value.kind {
        ExprKind::StringLiteral(name) => {
            let key = php_symbol_key(name.trim_start_matches('\\'));
            let Some(declared) = cx
                .checker
                .fn_decls
                .keys()
                .find(|candidate| php_symbol_key(candidate) == key)
                .cloned()
            else {
                return Ok(());
            };
            let spelled = name.trim_start_matches('\\');
            if spelled != declared {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "{}(): handler '{spelled}' must match the declared spelling '{declared}' (dynamic callables resolve case-sensitively)",
                        cx.name
                    ),
                ));
            }
            let decl = &cx.checker.fn_decls[&declared];
            let count = if decl.variadic.is_some() {
                hints.len()
            } else {
                decl.params.len().min(hints.len())
            };
            let args = dummy_args(count, &mut env);
            Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified(declared),
                    args,
                },
                span,
            )
        }
        ExprKind::ArrayLiteral(elements) if elements.len() == 2 => {
            let ExprKind::StringLiteral(method) = &elements[1].kind else {
                return Ok(());
            };
            let PhpType::Object(class) = cx.checker.infer_type(&elements[0], cx.env)?.codegen_repr()
            else {
                return Ok(());
            };
            let Some(sig) = method_signature(cx, &class, method) else {
                return Ok(());
            };
            let count = if sig.variadic.is_some() {
                hints.len()
            } else {
                regular_param_count(&sig).min(hints.len())
            };
            let args = dummy_args(count, &mut env);
            Expr::new(
                ExprKind::MethodCall {
                    object: Box::new(elements[0].clone()),
                    method: method.clone(),
                    args,
                },
                span,
            )
        }
        ExprKind::FirstClassCallable(target) => {
            let sig = cx.checker.resolve_first_class_callable_sig(target, span, &env)?;
            let count = if sig.variadic.is_some() {
                hints.len()
            } else {
                regular_param_count(&sig).min(hints.len())
            };
            let args = dummy_args(count, &mut env);
            cx.checker
                .specialize_first_class_callable_target(target, &args, span, &env)?;
            return Ok(());
        }
        _ => return Ok(()),
    };
    cx.checker.infer_type(&call, &env)?;
    Ok(())
}

/// Finds `method` on `class` or one of its ancestors.
fn method_signature(cx: &BuiltinCheckCtx, class: &str, method: &str) -> Option<FunctionSig> {
    let method_key = php_symbol_key(method);
    let mut current = Some(class.to_string());
    while let Some(name) = current {
        let info = cx.checker.classes.get(&name)?;
        if let Some(sig) = info.methods.get(&method_key) {
            return Some(sig.clone());
        }
        current = info.parent.clone();
    }
    None
}

/// Lowers the setter to one call of its prelude twin with the normalized operands.
pub(super) fn lower_setter(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
    spec: &SetterSpec,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let expected = spec.handlers.len() + 1;
    if call.operands.len() != expected {
        return Err(BuiltinLoweringError::new(format!(
            "{} lowering expected {expected} operands but received {}",
            call.name,
            call.operands.len()
        )));
    }
    Ok(ctx.emit_user_call(
        spec.helper,
        call.operands.to_vec(),
        PhpType::Bool,
        Some(call.span),
    ))
}
