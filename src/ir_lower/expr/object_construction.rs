//! Purpose:
//! Object construction, clone, and ReflectionParameter constructor operands.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers fixed-class object construction with constructor-specific allocation rules.
pub(super) fn lower_new_object(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &Name,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    if php_symbol_key(class_name.as_str().trim_start_matches('\\')) == "reflectionclass" {
        if let Some(operands) = lower_reflection_class_constructor_operands(ctx, args) {
            let php_type = PhpType::Object(class_name.as_str().to_string());
            return emit_fixed_object_new(ctx, class_name.as_str(), operands, php_type, expr.span);
        }
    }
    if php_symbol_key(class_name.as_str().trim_start_matches('\\')) == "reflectionparameter" {
        if let Some(operands) = lower_reflection_parameter_constructor_operands(ctx, args) {
            let php_type = PhpType::Object(class_name.as_str().to_string());
            return emit_fixed_object_new(ctx, class_name.as_str(), operands, php_type, expr.span);
        }
    }
    if php_symbol_key(class_name.as_str().trim_start_matches('\\')) == "reflectionmethod" {
        if let Some(operands) = lower_reflection_method_constructor_operands(ctx, args) {
            let php_type = PhpType::Object(class_name.as_str().to_string());
            return emit_fixed_object_new(ctx, class_name.as_str(), operands, php_type, expr.span);
        }
    }
    if ctx.has_eval_barrier()
        && !ctx.classes.contains_key(class_name.as_str())
        && plain_positional_call_args(args)
    {
        let operands = lower_args_with_signature(ctx, None, args);
        let data = ctx.intern_class_name(class_name.as_str());
        return ctx.emit_value(
            Op::EvalObjectNew,
            operands,
            Some(Immediate::Data(data)),
            PhpType::Mixed,
            Op::EvalObjectNew.default_effects(),
            Some(expr.span),
        );
    }
    if date_object_constructor_base(ctx, class_name).is_some() {
        return emit_preallocated_date_object_new(ctx, class_name, args, expr.span);
    }
    let sig = constructor_signature(ctx, class_name).cloned();
    if sig.is_some()
        && ctx
            .classes
            .get(class_name.as_str().trim_start_matches('\\'))
            .is_some_and(|class_info| class_info.declaration_span.line > 0)
    {
        return emit_preallocated_user_object_new(
            ctx,
            class_name,
            args,
            sig.as_ref().expect("checked user constructor signature"),
            expr.span,
        );
    }
    let operands = lower_args_with_signature(ctx, sig.as_ref(), args);
    let php_type = PhpType::Object(class_name.as_str().to_string());
    emit_fixed_object_new(ctx, class_name.as_str(), operands, php_type, expr.span)
}

/// Allocates a fixed userland receiver before evaluating its constructor arguments.
///
/// PHP assigns the receiver's object identity before any positional, named, spread,
/// or defaulted argument expression runs. Defaults are installed by
/// `ObjectNewWithoutConstructor`; the ordinary method-call path then preserves the
/// shared argument planner, constructor body, ownership, and target ABI contracts.
fn emit_preallocated_user_object_new(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &Name,
    args: &[Expr],
    sig: &FunctionSig,
    span: Span,
) -> LoweredValue {
    let php_type = PhpType::Object(class_name.as_str().to_string());
    let class_data = ctx.intern_class_name(class_name.as_str());
    let object = ctx.emit_value(
        Op::ObjectNewWithoutConstructor,
        Vec::new(),
        Some(Immediate::Data(class_data)),
        php_type,
        Op::ObjectNewWithoutConstructor.default_effects(),
        Some(span),
    );
    let arg_values = lower_args_with_signature(ctx, Some(sig), args);
    if uses_internal_mysqli_constructor(ctx, class_name)
        && arg_values.len() > sig.params.len()
    {
        let given = arg_values.len();
        release_owned_call_arg_temporaries(
            ctx,
            &arg_values,
            None,
            &ReturnArgAlias::None,
            span,
        );
        emit_exception(
            ctx,
            "ArgumentCountError",
            &format!(
                "mysqli::__construct() expects at most 6 arguments, {} given",
                given
            ),
            span,
        );
        return object;
    }
    let abi_arg_count = if sig.variadic.is_some() {
        arg_values.len()
    } else {
        arg_values.len().min(sig.params.len())
    };
    let mut operands = Vec::with_capacity(abi_arg_count + 1);
    operands.push(object.value);
    operands.extend(arg_values[..abi_arg_count].iter().copied());
    let method_data = ctx.intern_string("__construct");
    ctx.emit_void(
        Op::MethodCall,
        operands,
        Some(Immediate::Data(method_data)),
        Op::MethodCall.default_effects(),
        Some(span),
    );
    release_owned_call_arg_temporaries(
        ctx,
        &arg_values,
        None,
        &ReturnArgAlias::None,
        span,
    );
    object
}

/// Returns whether a class uses mysqli's internal six-argument constructor contract.
fn uses_internal_mysqli_constructor(ctx: &LoweringContext<'_, '_>, class_name: &Name) -> bool {
    let class_name = class_name.as_str().trim_start_matches('\\');
    if php_symbol_key(class_name) == "mysqli" {
        return true;
    }
    ctx.classes
        .get(class_name)
        .and_then(|class_info| class_info.method_declaring_classes.get("__construct"))
        .is_some_and(|declaring| php_symbol_key(declaring) == "mysqli")
}

/// Returns the ext/date class that declares the effective inherited constructor.
///
/// Descendants that inherit an internal constructor use the same preallocated
/// object and overload path; descendants with a userland override do not.
fn date_object_constructor_base(
    ctx: &LoweringContext<'_, '_>,
    class_name: &Name,
) -> Option<&'static str> {
    const BASES: [&str; 5] = [
        "DateTime",
        "DateTimeImmutable",
        "DateTimeZone",
        "DateInterval",
        "DatePeriod",
    ];
    let class_name = class_name.as_str().trim_start_matches('\\');
    if let Some(base) = BASES
        .iter()
        .copied()
        .find(|base| php_symbol_key(class_name) == php_symbol_key(base))
    {
        return Some(base);
    }
    let class_info = ctx.classes.get(class_name)?;
    if let Some(declaring_class) = class_info
        .method_declaring_classes
        .get("__construct")
    {
        return BASES
            .iter()
            .copied()
            .find(|base| php_symbol_key(declaring_class) == php_symbol_key(base));
    }
    BASES
        .iter()
        .copied()
        .find(|base| class_extends_class(ctx, class_name, base))
}

/// Allocates an ext/date object before lowering arguments, then invokes its constructor.
fn emit_preallocated_date_object_new(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &Name,
    args: &[Expr],
    span: Span,
) -> LoweredValue {
    let php_type = PhpType::Object(class_name.as_str().to_string());
    let class_data = ctx.intern_class_name(class_name.as_str());
    let object = ctx.emit_value(
        Op::ObjectNewWithoutConstructor,
        Vec::new(),
        Some(Immediate::Data(class_data)),
        php_type,
        Op::ObjectNewWithoutConstructor.default_effects(),
        Some(span),
    );
    let date_constructor_base = date_object_constructor_base(ctx, class_name)
        .expect("preallocated ext/date object must have a constructor base");
    let sig = ctx
        .classes
        .get(date_constructor_base)
        .and_then(|class_info| class_info.methods.get("__construct"))
        .cloned();
    if sig.is_some() {
        let is_date_period_constructor = date_constructor_base == "DatePeriod";
        let date_period_slots = is_date_period_constructor.then(|| {
            crate::types::call_args::fixed_parameter_slots(
                args,
                &["start", "interval", "end", "options"],
            )
        });
        if emit_preallocated_date_constructor_argument_array(
            ctx,
            object.value,
            args,
            span,
            is_date_period_constructor,
        ) {
            return object;
        }
        if let Some(crate::types::call_args::FixedParameterSlots::Unknown(name)) =
            &date_period_slots
        {
            let evaluated_args = lower_args(ctx, args);
            release_owned_call_arg_temporaries(
                ctx,
                &evaluated_args,
                None,
                &ReturnArgAlias::None,
                span,
            );
            emit_exception(
                ctx,
                "Error",
                &format!("Unknown named parameter ${}", name),
                span,
            );
            return object;
        }
        if let Some(crate::types::call_args::FixedParameterSlots::Missing(parameter_index)) =
            &date_period_slots
        {
            let evaluated_args = lower_args(ctx, args);
            release_owned_call_arg_temporaries(
                ctx,
                &evaluated_args,
                None,
                &ReturnArgAlias::None,
                span,
            );
            emit_exception(
                ctx,
                "ArgumentCountError",
                &date_period_missing_argument_message(*parameter_index),
                span,
            );
            return object;
        }
        let uses_date_period_string_overload = is_date_period_constructor
            && (matches!(
                date_period_slots.as_ref(),
                Some(crate::types::call_args::FixedParameterSlots::Contiguous(1 | 2))
            ) || (matches!(
                date_period_slots.as_ref(),
                Some(crate::types::call_args::FixedParameterSlots::NoNamedArguments)
            ) && matches!(static_call_argument_count(args), Some(1 | 2))));
        if uses_date_period_string_overload {
            let helper_key = php_symbol_key("__elephc_initialize_from_iso8601_string");
            let helper_sig = ctx
                .classes
                .get(date_constructor_base)
                .and_then(|class_info| class_info.methods.get(&helper_key))
                .cloned()
                .expect("DatePeriod string initializer signature must exist");
            let lowering_sig = date_constructor_dynamic_string_signature(&helper_sig);
            let mut arg_values = lower_args_with_signature(ctx, Some(&lowering_sig), args);
            let source_line = emit_i64_at_span(ctx, span.line as i64, span);
            if let Some(line) = arg_values.get_mut(2) {
                *line = source_line.value;
            } else {
                arg_values.push(source_line.value);
            }
            coerce_date_constructor_dynamic_string_args(
                ctx,
                "DatePeriod",
                &mut arg_values,
                span,
            );
            coerce_date_period_runtime_option_at(ctx, &mut arg_values, 1, span);
            let mut operands = Vec::with_capacity(arg_values.len() + 1);
            operands.push(object.value);
            operands.extend(arg_values.iter().copied());
            let method_data = ctx.intern_string(&helper_key);
            ctx.emit_void(
                Op::MethodCall,
                operands,
                Some(Immediate::Data(method_data)),
                Op::MethodCall.default_effects(),
                Some(span),
            );
            release_owned_call_arg_temporaries(
                ctx,
                &arg_values,
                None,
                &ReturnArgAlias::None,
                span,
            );
            return object;
        }
        if let Some((exception_class, message)) =
            date_constructor_static_arity_error(date_constructor_base, args)
        {
            let evaluated_args = lower_args(ctx, args);
            release_owned_call_arg_temporaries(
                ctx,
                &evaluated_args,
                None,
                &ReturnArgAlias::None,
                span,
            );
            emit_exception(ctx, exception_class, &message, span);
            return object;
        }
        let spread_overflow_error =
            is_date_period_constructor.then_some(DATE_PERIOD_CONSTRUCTOR_OVERLOAD_ERROR);
        let lowering_sig = date_constructor_dynamic_string_signature(
            sig.as_ref()
                .expect("preallocated date constructor signature must exist"),
        );
        let mut arg_values = lower_args_with_signature_and_spread_overflow(
            ctx,
            Some(&lowering_sig),
            args,
            spread_overflow_error,
        );
        if sig
            .as_ref()
            .and_then(|signature| signature.params.first())
            .is_some_and(|(_, param_type)| param_type.codegen_repr() == PhpType::Str)
        {
            coerce_date_constructor_dynamic_string_args(
                ctx,
                date_constructor_base,
                &mut arg_values,
                span,
            );
        }
        validate_preallocated_date_constructor_args(
            ctx,
            date_constructor_base,
            &mut arg_values,
            span,
        );
        let mut operands = Vec::with_capacity(arg_values.len() + 1);
        operands.push(object.value);
        operands.extend(arg_values.iter().copied());
        let method_data = ctx.intern_string("__construct");
        ctx.emit_void(
            Op::MethodCall,
            operands,
            Some(Immediate::Data(method_data)),
            Op::MethodCall.default_effects(),
            Some(span),
        );
        release_owned_call_arg_temporaries(
            ctx,
            &arg_values,
            None,
            &ReturnArgAlias::None,
            span,
        );
    }
    object
}

/// Returns php-src's internal-constructor arity error for statically countable arguments.
fn date_constructor_static_arity_error(
    constructor_base: &str,
    args: &[Expr],
) -> Option<(&'static str, String)> {
    let given = static_call_argument_count(args)?;
    match php_symbol_key(constructor_base).as_str() {
        "datetime" | "datetimeimmutable" if given > 2 => Some((
            "ArgumentCountError",
            format!(
                "{}::__construct() expects at most 2 arguments, {} given",
                constructor_base, given
            ),
        )),
        "datetimezone" | "dateinterval" if given != 1 => Some((
            "ArgumentCountError",
            format!(
                "{}::__construct() expects exactly 1 argument, {} given",
                constructor_base, given
            ),
        )),
        "dateperiod" if given == 0 || given > 4 => Some((
            "TypeError",
            DATE_PERIOD_CONSTRUCTOR_OVERLOAD_ERROR.to_string(),
        )),
        _ => None,
    }
}

/// Counts ordinary arguments and statically indexed spread literals, or returns dynamic.
fn static_call_argument_count(args: &[Expr]) -> Option<usize> {
    let mut count = 0usize;
    for arg in args {
        match &arg.kind {
            ExprKind::Spread(inner) => {
                let ExprKind::ArrayLiteral(items) = &inner.kind else {
                    return None;
                };
                count = count.saturating_add(items.len());
            }
            _ => count = count.saturating_add(1),
        }
    }
    Some(count)
}

/// Normalizes ext/date constructor arguments at PHP's source-order error boundary.
///
/// DatePeriod always uses its runtime overload dispatcher because argument count alone cannot
/// distinguish the deprecated string form from the object forms. Other date classes need the
/// incremental argument container only when an unpack is present.
fn emit_preallocated_date_constructor_argument_array(
    ctx: &mut LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    args: &[Expr],
    span: Span,
    force_runtime_overload: bool,
) -> bool {
    let has_spread = args
        .iter()
        .any(|arg| matches!(arg.kind, ExprKind::Spread(_)));
    if !force_runtime_overload && !has_spread {
        return false;
    }

    let begin_method = ctx.intern_string("__elephc_begin_argument_array");
    ctx.emit_void(
        Op::MethodCall,
        vec![object],
        Some(Immediate::Data(begin_method)),
        Op::MethodCall.default_effects(),
        Some(span),
    );

    for arg in args {
        let (kind, name, value_expr) = match &arg.kind {
            ExprKind::Spread(inner) => (1, "", inner.as_ref()),
            ExprKind::NamedArg { name, value } => (2, name.as_str(), value.as_ref()),
            _ => (0, "", arg),
        };
        let kind = emit_i64_at_span(ctx, kind, arg.span);
        let name_expr = Expr::new(ExprKind::StringLiteral(name.to_string()), arg.span);
        let name = lower_string_literal(ctx, name, &name_expr);
        let value = lower_expr(ctx, value_expr);
        let value = if ctx.builder.value_php_type(value.value).codegen_repr() == PhpType::Mixed {
            value
        } else {
            ctx.box_value_as_mixed(value, PhpType::Mixed, Some(arg.span))
        };
        let append_method = ctx.intern_string("__elephc_append_argument_chunk");
        ctx.emit_void(
            Op::MethodCall,
            vec![object, kind.value, name.value, value.value],
            Some(Immediate::Data(append_method)),
            Op::MethodCall.default_effects(),
            Some(arg.span),
        );
        release_owned_call_arg_temporaries(
            ctx,
            &[value.value],
            None,
            &ReturnArgAlias::None,
            arg.span,
        );
    }

    let finish_method = ctx.intern_string("__elephc_finish_argument_array");
    let mut finish_operands = vec![object];
    if force_runtime_overload {
        finish_operands.push(emit_i64_at_span(ctx, span.line as i64, span).value);
    }
    ctx.emit_void(
        Op::MethodCall,
        finish_operands,
        Some(Immediate::Data(finish_method)),
        Op::MethodCall.default_effects(),
        Some(span),
    );
    true
}

const DATE_PERIOD_CONSTRUCTOR_OVERLOAD_ERROR: &str =
    "DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or \
(DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments";

/// Formats php-src's explicit-hole error for DatePeriod's unknown-default parameters.
fn date_period_missing_argument_message(parameter_index: usize) -> String {
    const PARAMETER_NAMES: [&str; 4] = ["start", "interval", "end", "options"];
    let parameter_name = PARAMETER_NAMES
        .get(parameter_index)
        .copied()
        .unwrap_or("unknown");
    if parameter_index == 0 {
        return "DatePeriod::__construct(): Argument #1 ($start) not passed".to_string();
    }
    format!(
        "DatePeriod::__construct(): Argument #{} (${}) must be passed explicitly, because the default value is not known",
        parameter_index + 1,
        parameter_name
    )
}

/// Replaces a leading string parameter with `mixed` so runtime guards see spread values intact.
fn date_constructor_dynamic_string_signature(signature: &FunctionSig) -> FunctionSig {
    let mut signature = signature.clone();
    if let Some((_, param_type)) = signature.params.first_mut() {
        if param_type.codegen_repr() == PhpType::Str {
            *param_type = PhpType::Mixed;
        }
    }
    signature
}

/// Applies php-src's weak string coercion and exact invalid-type diagnostics.
fn coerce_date_constructor_dynamic_string_args(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    args: &mut [crate::ir::ValueId],
    span: Span,
) {
    let Some(value) = args.first().copied() else {
        return;
    };
    let value_type = ctx.builder.value_php_type(value).codegen_repr();
    let date_period = php_symbol_key(class_name) == "dateperiod";
    if date_period && value_type == PhpType::Void {
        emit_date_period_null_string_deprecation(ctx, span);
        let empty_expr = Expr::new(ExprKind::StringLiteral(String::new()), span);
        args[0] = lower_string_literal(ctx, "", &empty_expr).value;
        return;
    }
    if !matches!(value_type, PhpType::Mixed | PhpType::Union(_)) {
        return;
    }
    let (prefix, fixed_error) = match php_symbol_key(class_name).as_str() {
        "datetime" => (
            "DateTime::__construct(): Argument #1 ($datetime) must be of type string, ",
            "",
        ),
        "datetimeimmutable" => (
            "DateTimeImmutable::__construct(): Argument #1 ($datetime) must be of type string, ",
            "",
        ),
        "datetimezone" => (
            "DateTimeZone::__construct(): Argument #1 ($timezone) must be of type string, ",
            "",
        ),
        "dateinterval" => (
            "DateInterval::__construct(): Argument #1 ($duration) must be of type string, ",
            "",
        ),
        "dateperiod" => ("", DATE_PERIOD_CONSTRUCTOR_OVERLOAD_ERROR),
        _ => return,
    };
    let prefix_expr = Expr::new(ExprKind::StringLiteral(prefix.to_string()), span);
    let prefix_value = lower_string_literal(ctx, prefix, &prefix_expr);
    let fixed_expr = Expr::new(ExprKind::StringLiteral(fixed_error.to_string()), span);
    let fixed_value = lower_string_literal(ctx, fixed_error, &fixed_expr);
    let method_name = if date_period {
        "DatePeriod::__elephc_weak_string_argument"
    } else {
        "DateTime::__elephc_weak_string_argument"
    };
    let method_data = ctx.intern_string(method_name);
    let operands = if date_period {
        let line = emit_i64_at_span(ctx, span.line as i64, span);
        vec![value, line.value]
    } else {
        vec![value, prefix_value.value, fixed_value.value]
    };
    let coerced = ctx.emit_value(
        Op::StaticMethodCall,
        operands,
        Some(Immediate::Data(method_data)),
        PhpType::Str,
        Op::StaticMethodCall.default_effects(),
        Some(span),
    );
    ctx.transfer_call_arg_temp_cleanup(value, coerced.value);
    args[0] = coerced.value;
}

/// Emits php-src's weak-null deprecation before DatePeriod's string-overload warning.
fn emit_date_period_null_string_deprecation(ctx: &mut LoweringContext<'_, '_>, span: Span) {
    let message_text = "\nDeprecated: DatePeriod::__construct(): Passing null to parameter #1 ($start) of type string is deprecated";
    let message_expr = Expr::new(
        ExprKind::StringLiteral(message_text.to_string()),
        span,
    );
    let message = lower_string_literal(ctx, message_text, &message_expr);
    let line = emit_i64_at_span(ctx, span.line as i64, span);
    let level = emit_i64_at_span(ctx, 8192, span);
    let warning = emit_builtin_call_value(
        ctx,
        "__elephc_diag_warning",
        vec![message.value, line.value, level.value],
        PhpType::Void,
        span,
        None,
    );
    let _ = warning;
}

/// Rejects dynamic DatePeriod spread values before the direct ABI unboxes object parameters.
fn validate_preallocated_date_constructor_args(
    ctx: &mut LoweringContext<'_, '_>,
    constructor_base: &str,
    args: &mut [crate::ir::ValueId],
    span: Span,
) {
    match constructor_base {
        "DateTime" | "DateTimeImmutable" => {
            if let Some(timezone) = args.get(1).copied() {
                emit_runtime_nullable_object_argument_guard(
                    ctx,
                    timezone,
                    "DateTimeZone",
                    &format!(
                        "{}::__construct(): Argument #2 ($timezone) must be of type ?DateTimeZone, ",
                        constructor_base
                    ),
                    span,
                );
            }
        }
        "DatePeriod" => {
            coerce_date_period_runtime_option_at(ctx, args, 3, span);
            if let Some(start) = args.first().copied() {
                emit_runtime_object_argument_guard(
                    ctx,
                    start,
                    "DateTimeInterface",
                    DATE_PERIOD_CONSTRUCTOR_OVERLOAD_ERROR,
                    span,
                );
            }
            if let Some(interval) = args.get(1).copied() {
                emit_runtime_object_argument_guard(
                    ctx,
                    interval,
                    "DateInterval",
                    DATE_PERIOD_CONSTRUCTOR_OVERLOAD_ERROR,
                    span,
                );
            }
        }
        _ => {}
    }
}

/// Coerces one DatePeriod dynamic `$options` slot or throws the overload TypeError.
fn coerce_date_period_runtime_option_at(
    ctx: &mut LoweringContext<'_, '_>,
    args: &mut [crate::ir::ValueId],
    index: usize,
    span: Span,
) {
    let Some(value) = args.get(index).copied() else {
        return;
    };
    if !matches!(
        ctx.builder.value_php_type(value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return;
    }
    let method_data = ctx.intern_string("DatePeriod::__elephc_weak_options");
    let coerced = ctx.emit_value(
        Op::StaticMethodCall,
        vec![value],
        Some(Immediate::Data(method_data)),
        PhpType::Int,
        Op::StaticMethodCall.default_effects(),
        Some(span),
    );
    ctx.transfer_call_arg_temp_cleanup(value, coerced.value);
    args[index] = coerced.value;
}

/// Accepts `null` or the requested object class and reports the concrete rejected PHP type.
fn emit_runtime_nullable_object_argument_guard(
    ctx: &mut LoweringContext<'_, '_>,
    value: crate::ir::ValueId,
    expected_class: &str,
    message_prefix: &str,
    span: Span,
) {
    if !matches!(
        ctx.builder.value_php_type(value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return;
    }
    let valid = ctx
        .builder
        .create_named_block("date.constructor.nullable.valid", Vec::new());
    let object_check = ctx
        .builder
        .create_named_block("date.constructor.nullable.object", Vec::new());
    let invalid = ctx
        .builder
        .create_named_block("date.constructor.nullable.invalid", Vec::new());
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: valid,
        then_args: Vec::new(),
        else_target: object_check,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(object_check);
    let class_data = ctx.intern_class_name(expected_class);
    let matches = ctx.emit_value(
        Op::InstanceOf,
        vec![value],
        Some(Immediate::Data(class_data)),
        PhpType::Bool,
        Op::InstanceOf.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: matches.value,
        then_target: valid,
        then_args: Vec::new(),
        else_target: invalid,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(invalid);
    emit_runtime_argument_type_error_and_terminate(ctx, value, message_prefix, span);
    ctx.builder.position_at_end(valid);
}

/// Calls the hidden php-src-style runtime type formatter and closes the throwing branch.
pub(super) fn emit_runtime_argument_type_error_and_terminate(
    ctx: &mut LoweringContext<'_, '_>,
    value: crate::ir::ValueId,
    message_prefix: &str,
    span: Span,
) {
    let prefix_expr = Expr::new(ExprKind::StringLiteral(message_prefix.to_string()), span);
    let prefix = lower_string_literal(ctx, message_prefix, &prefix_expr);
    let method_data = ctx.intern_string("DateTime::__elephc_argument_type_error");
    ctx.emit_void(
        Op::StaticMethodCall,
        vec![value, prefix.value],
        Some(Immediate::Data(method_data)),
        Op::StaticMethodCall.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::Unreachable);
}

/// Throws a catchable `TypeError` unless a dynamic operand implements the required class.
fn emit_runtime_object_argument_guard(
    ctx: &mut LoweringContext<'_, '_>,
    value: crate::ir::ValueId,
    expected_class: &str,
    message: &str,
    span: Span,
) {
    if !matches!(
        ctx.builder.value_php_type(value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return;
    }
    let class_data = ctx.intern_class_name(expected_class);
    let matches = ctx.emit_value(
        Op::InstanceOf,
        vec![value],
        Some(Immediate::Data(class_data)),
        PhpType::Bool,
        Op::InstanceOf.default_effects(),
        Some(span),
    );
    let valid = ctx
        .builder
        .create_named_block("date.constructor.arg.valid", Vec::new());
    let invalid = ctx
        .builder
        .create_named_block("date.constructor.arg.invalid", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: matches.value,
        then_target: valid,
        then_args: Vec::new(),
        else_target: invalid,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(invalid);
    emit_exception_and_terminate(ctx, "TypeError", message, span);
    ctx.builder.position_at_end(valid);
}

/// Constructs and throws one catchable exception, then closes the non-returning EIR block.
pub(super) fn emit_exception_and_terminate(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    message: &str,
    span: Span,
) {
    emit_exception(ctx, class_name, message, span);
    ctx.builder.terminate(Terminator::Unreachable);
}

/// Constructs and throws one catchable exception while leaving structural cleanup lowering open.
pub(super) fn emit_exception(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    message: &str,
    span: Span,
) {
    let error_expr = Expr::new(
        ExprKind::NewObject {
            class_name: Name::unqualified(class_name),
            args: vec![Expr::new(
                ExprKind::StringLiteral(message.to_string()),
                span,
            )],
        },
        span,
    );
    let error = lower_expr(ctx, &error_expr);
    ctx.emit_void(
        Op::ThrowException,
        vec![error.value],
        None,
        Op::ThrowException.default_effects(),
        Some(span),
    );
}

/// Emits fixed-class object construction and releases owned constructor argument temporaries.
///
/// A newly allocated object cannot alias a constructor argument. The constructor has already
/// retained or copied every argument it keeps by the time `ObjectNew` returns, so the caller's
/// owning temporary references can be dropped without the general call-result alias guard.
pub(super) fn emit_fixed_object_new(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    operands: Vec<ValueId>,
    php_type: PhpType,
    span: Span,
) -> LoweredValue {
    let data = ctx.intern_class_name(class_name);
    let object = ctx.emit_value(
        Op::ObjectNew,
        operands.clone(),
        Some(Immediate::Data(data)),
        php_type,
        Op::ObjectNew.default_effects(),
        Some(span),
    );
    release_owned_call_arg_temporaries(
        ctx,
        &operands,
        None,
        &ReturnArgAlias::None,
        span,
    );
    object
}

/// Lowers `ReflectionClass(object)` while preserving object operands for runtime class metadata.
pub(super) fn lower_reflection_class_constructor_operands(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Vec<ValueId>> {
    let reflected_arg = reflection_class_constructor_class_arg(ctx, args)?;
    let class_name = instance_callable_object_class(ctx, &reflected_arg)?;
    let lowered = lower_expr(ctx, &reflected_arg);
    if matches!(
        ctx.builder.value_php_type(lowered.value).codegen_repr(),
        PhpType::Object(_)
    ) {
        return Some(vec![lowered.value]);
    }
    if ctx.value_is_owning_temporary(lowered) {
        crate::ir_lower::ownership::release_if_owned(ctx, lowered, Some(reflected_arg.span));
    }
    let data = ctx.intern_class_name(&class_name);
    let value = ctx.emit_value(
        Op::ConstClassName,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Str,
        Op::ConstClassName.default_effects(),
        Some(reflected_arg.span),
    );
    Some(vec![value.value])
}

/// Lowers direct `ReflectionMethod` constructor operands to literal class and method names.
pub(super) fn lower_reflection_method_constructor_operands(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Vec<ValueId>> {
    let (class_arg, method_arg) = reflection_method_constructor_regular_args(ctx, args)?;
    Some(vec![
        lower_expr(ctx, &class_arg).value,
        lower_expr(ctx, &method_arg).value,
    ])
}

/// Lowers PHP `clone $object` to a shallow object-copy opcode and optional `__clone()` hook.
pub(super) fn lower_clone(ctx: &mut LoweringContext<'_, '_>, inner: &Expr, expr: &Expr) -> LoweredValue {
    let object = lower_expr(ctx, inner);
    let object_ty = ctx.builder.value_php_type(object.value);
    if matches!(object_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        let cloned = ctx.emit_value(
            Op::ObjectCloneShallow,
            vec![object.value],
            None,
            PhpType::Mixed,
            Op::ObjectCloneShallow.default_effects(),
            Some(expr.span),
        );
        release_owning_receiver_temporary(ctx, object, expr.span);
        return cloned;
    }
    let Some((class_name, false)) = singular_object_class(&object_ty) else {
        unreachable!("clone expressions must be type-checked as non-null objects before lowering");
    };
    let class_name = class_name.to_string();
    let data = ctx.intern_class_name(&class_name);
    let result_ty = PhpType::Object(class_name.clone());
    let cloned = ctx.emit_value(
        Op::ObjectCloneShallow,
        vec![object.value],
        Some(Immediate::Data(data)),
        result_ty,
        Op::ObjectCloneShallow.default_effects(),
        Some(expr.span),
    );
    if class_method_signature(ctx, &class_name, &php_symbol_key("__clone")).is_some() {
        // Method-call lowering releases an owning temporary receiver after the
        // call. Keep the clone expression's original ownership alive while the
        // hook consumes a separately acquired receiver reference.
        let hook_receiver =
            crate::ir_lower::ownership::acquire_if_refcounted(ctx, cloned, Some(expr.span));
        lower_method_call_with_receiver(
            ctx,
            hook_receiver,
            "__clone",
            &[],
            Op::MethodCall,
            expr,
        );
    }
    // `clone` borrows its source while copying it. Array/hash reads and call results can own a
    // one-shot source temporary; once the optional `__clone()` hook has completed, that source
    // no longer participates in the expression and must be released.
    release_owning_receiver_temporary(ctx, object, expr.span);
    cloned
}

/// Metadata operand source for direct `ReflectionParameter` constructor lowering.
pub(super) enum ReflectionParameterConstructorOperand {
    Expr(Expr),
    ClassName { name: String, span: Span },
    ObjectExpr { expr: Expr, span: Span },
}

/// Lowers validated `ReflectionParameter` constructor arguments into metadata operands.
///
/// Method targets lower as `[class, method, parameter]`; function targets lower
/// as `[function, parameter]`.
pub(super) fn lower_reflection_parameter_constructor_operands(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Vec<ValueId>> {
    let arg_exprs = reflection_parameter_constructor_arg_exprs(ctx, args)?;
    Some(
        arg_exprs
            .iter()
            .map(|arg| lower_reflection_parameter_constructor_operand(ctx, arg))
            .collect(),
    )
}

/// Lowers one direct `ReflectionParameter` metadata operand.
pub(super) fn lower_reflection_parameter_constructor_operand(
    ctx: &mut LoweringContext<'_, '_>,
    operand: &ReflectionParameterConstructorOperand,
) -> ValueId {
    match operand {
        ReflectionParameterConstructorOperand::Expr(expr) => lower_expr(ctx, expr).value,
        ReflectionParameterConstructorOperand::ObjectExpr { expr, span } => {
            let object = lower_expr(ctx, expr);
            let class_name = reflection_parameter_lowered_object_class_name(ctx, object.value)
                .expect("ReflectionParameter object target must be type-checked as a known object");
            if ctx.value_is_owning_temporary(object) {
                crate::ir_lower::ownership::release_if_owned(ctx, object, Some(*span));
            }
            emit_reflection_parameter_class_name_operand(ctx, &class_name, *span)
        }
        ReflectionParameterConstructorOperand::ClassName { name, span } => {
            emit_reflection_parameter_class_name_operand(ctx, name, *span)
        }
    }
}

/// Emits one class-name operand for direct `ReflectionParameter` metadata.
pub(super) fn emit_reflection_parameter_class_name_operand(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    span: Span,
) -> ValueId {
    let data = ctx.intern_class_name(name);
    ctx.emit_value(
        Op::ConstClassName,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Str,
        Op::ConstClassName.default_effects(),
        Some(span),
    )
    .value
}

/// Returns metadata operand expressions from a normalized static `ReflectionParameter` call.
pub(super) fn reflection_parameter_constructor_arg_exprs(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Vec<ReflectionParameterConstructorOperand>> {
    let args = expand_static_call_spread_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    let (target, parameter) = if crate::types::call_args::has_named_args(&args) {
        let sig = ctx
            .classes
            .get("ReflectionParameter")
            .and_then(|class_info| class_info.methods.get("__construct"))?;
        let call_span = args
            .first()
            .map(|arg| arg.span)
            .unwrap_or_else(crate::span::Span::dummy);
        let plan =
            crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
                sig,
                &args,
                call_span,
                crate::types::call_args::regular_param_count(sig),
                false,
                true,
                &assoc_spread_sources(ctx, &args),
            )
            .ok()?;
        if plan.has_spread_args() {
            return None;
        }
        (
            planned_regular_arg_expr(plan.regular_args.first()?)?.clone(),
            planned_regular_arg_expr(plan.regular_args.get(1)?)?.clone(),
        )
    } else {
        (args.first()?.clone(), args.get(1)?.clone())
    };
    match &target.kind {
        ExprKind::ArrayLiteral(items) if items.len() == 2 => {
            let owner = reflection_parameter_method_owner_operand(ctx, &items[0])?;
            Some(vec![
                owner,
                ReflectionParameterConstructorOperand::Expr(items[1].clone()),
                ReflectionParameterConstructorOperand::Expr(parameter),
            ])
        }
        ExprKind::StringLiteral(_) => Some(vec![
            ReflectionParameterConstructorOperand::Expr(target),
            ReflectionParameterConstructorOperand::Expr(parameter),
        ]),
        _ => None,
    }
}

/// Returns the static class-name operand for a ReflectionParameter method target.
pub(super) fn reflection_parameter_method_owner_operand(
    ctx: &LoweringContext<'_, '_>,
    owner: &Expr,
) -> Option<ReflectionParameterConstructorOperand> {
    match &owner.kind {
        ExprKind::StringLiteral(name) => Some(ReflectionParameterConstructorOperand::ClassName {
            name: name.clone(),
            span: owner.span,
        }),
        ExprKind::ClassConstant { receiver } => {
            static_receiver_class_name(ctx, receiver).map(|name| {
                ReflectionParameterConstructorOperand::ClassName {
                    name,
                    span: owner.span,
                }
            })
        }
        ExprKind::Variable(name) => {
            let PhpType::Object(class_name) = ctx.local_type(name).codegen_repr() else {
                return None;
            };
            if class_name.is_empty() {
                return None;
            }
            Some(ReflectionParameterConstructorOperand::ClassName {
                name: class_name,
                span: owner.span,
            })
        }
        ExprKind::This => {
            ctx.current_class
                .clone()
                .map(|name| ReflectionParameterConstructorOperand::ClassName {
                    name,
                    span: owner.span,
                })
        }
        _ => Some(ReflectionParameterConstructorOperand::ObjectExpr {
            expr: owner.clone(),
            span: owner.span,
        }),
    }
}

/// Returns the concrete class name from a lowered object target.
pub(super) fn reflection_parameter_lowered_object_class_name(
    ctx: &LoweringContext<'_, '_>,
    value: ValueId,
) -> Option<String> {
    let PhpType::Object(class_name) = ctx.builder.value_php_type(value).codegen_repr() else {
        return None;
    };
    if class_name.is_empty() || !ctx.classes.contains_key(class_name.as_str()) {
        return None;
    }
    Some(class_name)
}

/// Date/time classes whose dynamic construction must preserve php-src allocation order.
const DYNAMIC_DATETIME_NEW_CLASSES: &[&str] = &[
    "DateTime",
    "DateTimeImmutable",
    "DateTimeZone",
    "DateInterval",
    "DatePeriod",
];

/// Lowers PHP `new $class(...)`, specializing ext/date candidates before the generic path.
pub(super) fn lower_new_dynamic(
    ctx: &mut LoweringContext<'_, '_>,
    name_expr: &Expr,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let args = expand_static_call_spread_args(args);
    if let Some(value) = lower_new_dynamic_planned_dispatch(ctx, name_expr, &args, expr) {
        return value;
    }
    let class_name = lower_expr(ctx, name_expr);
    if ctx.builder.value_php_type(class_name.value).codegen_repr() == PhpType::Str
        && DYNAMIC_DATETIME_NEW_CLASSES
            .iter()
            .any(|candidate| ctx.classes.contains_key(*candidate))
    {
        return lower_dynamic_datetime_object_new(ctx, class_name, &args, expr);
    }
    emit_generic_dynamic_object_new(ctx, class_name, &args, expr)
}

/// Dispatches a string-valued dynamic class name across concrete ext/date constructors.
///
/// Each matching branch uses fixed-class lowering, which allocates the object before lowering
/// constructor arguments and applies the concrete signature's named/default argument plan.
fn lower_dynamic_datetime_object_new(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: LoweredValue,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let temp_name = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let merge = ctx
        .builder
        .create_named_block("dynamic.datetime.new.merge", Vec::new());

    for candidate in DYNAMIC_DATETIME_NEW_CLASSES {
        if !dynamic_datetime_constructor_accepts_args(ctx, candidate, args) {
            continue;
        }
        let matched = ctx
            .builder
            .create_named_block("dynamic.datetime.new.match", Vec::new());
        let leading_slash_check = ctx
            .builder
            .create_named_block("dynamic.datetime.new.leading_slash", Vec::new());
        let next = ctx
            .builder
            .create_named_block("dynamic.datetime.new.next", Vec::new());

        let direct_match =
            emit_dynamic_datetime_class_name_match(ctx, class_name.value, candidate, expr.span);
        ctx.builder.terminate(Terminator::CondBr {
            cond: direct_match.value,
            then_target: matched,
            then_args: Vec::new(),
            else_target: leading_slash_check,
            else_args: Vec::new(),
        });

        ctx.builder.position_at_end(leading_slash_check);
        let leading_slash_candidate = format!("\\{}", candidate);
        let leading_slash_match = emit_dynamic_datetime_class_name_match(
            ctx,
            class_name.value,
            &leading_slash_candidate,
            expr.span,
        );
        ctx.builder.terminate(Terminator::CondBr {
            cond: leading_slash_match.value,
            then_target: matched,
            then_args: Vec::new(),
            else_target: next,
            else_args: Vec::new(),
        });

        ctx.builder.position_at_end(matched);
        let candidate_name = Name::unqualified(*candidate);
        let object = lower_dynamic_datetime_candidate_new(ctx, &candidate_name, args, expr);
        if !ctx.builder.insertion_block_is_terminated() {
            let boxed = ctx.box_value_as_mixed(object, PhpType::Mixed, Some(expr.span));
            store_value_into_temp(ctx, &temp_name, PhpType::Mixed, boxed, expr.span);
            branch_to(ctx, merge);
        }

        ctx.builder.position_at_end(next);
    }

    let fallback = emit_generic_dynamic_object_new(ctx, class_name, args, expr);
    store_value_into_temp(ctx, &temp_name, PhpType::Mixed, fallback, expr.span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    take_owned_temp(ctx, &temp_name, expr.span)
}

/// Lowers one matched ext/date candidate, including DatePeriod's string overload.
fn lower_dynamic_datetime_candidate_new(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &Name,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    lower_new_object(ctx, class_name, args, expr)
}

/// Returns whether one concrete ext/date constructor can accept this dynamic argument shape.
fn dynamic_datetime_constructor_accepts_args(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    args: &[Expr],
) -> bool {
    let Some(signature) = ctx
        .classes
        .get(class_name)
        .and_then(|class_info| class_info.methods.get("__construct"))
    else {
        return false;
    };
    let regular_param_count = crate::types::call_args::regular_param_count(signature);
    if !args.iter().any(is_spread_arg) {
        let required_param_count = signature
            .defaults
            .iter()
            .take(regular_param_count)
            .filter(|default| default.is_none())
            .count();
        if signature.variadic.is_none()
            && (args.len() < required_param_count || args.len() > regular_param_count)
        {
            // Keep the concrete branch so internal ext/date constructors can emit
            // php-src's runtime arity exception instead of falling through to the
            // generic dynamic-new path.
            return true;
        }
    }
    let call_span = args
        .first()
        .map(|arg| arg.span)
        .unwrap_or_else(Span::dummy);
    crate::types::call_args::plan_call_args_with_regular_param_count_and_assoc_spreads(
        signature,
        args,
        call_span,
        regular_param_count,
        false,
        true,
        &assoc_spread_sources(ctx, args),
    )
    .is_ok()
}

/// Compares one evaluated dynamic class string against a concrete class name.
fn emit_dynamic_datetime_class_name_match(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: ValueId,
    candidate: &str,
    span: Span,
) -> LoweredValue {
    let candidate_expr = Expr::new(ExprKind::StringLiteral(candidate.to_string()), span);
    let candidate_value = lower_string_literal(ctx, candidate, &candidate_expr);
    let comparison = ctx.emit_value(
        Op::RuntimeCall,
        vec![class_name, candidate_value.value],
        Some(Immediate::RuntimeCall(
            crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::Strcasecmp),
        )),
        PhpType::Int,
        crate::ir::RuntimeFnId::Strcasecmp.effects(),
        Some(span),
    );
    let zero = emit_i64_at_span(ctx, 0, span);
    ctx.emit_value(
        Op::ICmp,
        vec![comparison.value, zero.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Eq)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    )
}

/// Emits the existing generic dynamic-new opcode from an already evaluated class name.
fn emit_generic_dynamic_object_new(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: LoweredValue,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let mut operands = vec![class_name.value];
    let uses_runtime_arg_container =
        args.iter().any(is_spread_arg) || crate::types::call_args::has_named_args(args);
    if uses_runtime_arg_container {
        let arg_container = lower_untyped_descriptor_invoker_arg_container(ctx, args, expr.span)
            .expect("dynamic constructor arguments always have a runtime container form");
        operands.push(arg_container.value);
    } else {
        operands.extend(lower_args(ctx, args));
    }
    ctx.emit_value(
        Op::DynamicObjectNewMixed,
        operands,
        uses_runtime_arg_container.then_some(Immediate::Bool(true)),
        PhpType::Mixed,
        Op::DynamicObjectNewMixed.default_effects(),
        Some(expr.span),
    )
}

/// Lowers dynamic object construction.
pub(super) fn lower_new_dynamic_object(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &Expr,
    fallback_class: &Name,
    required_parent: &Name,
    args: &[Expr],
    expr: &Expr,
) -> LoweredValue {
    let mut operands = vec![lower_expr(ctx, class_name).value];
    operands.extend(lower_args(ctx, args));
    let name = format!("{}|{}", fallback_class.as_str(), required_parent.as_str());
    let data = ctx.intern_class_name(&name);
    ctx.emit_value(
        Op::DynamicObjectNew,
        operands,
        Some(Immediate::Data(data)),
        PhpType::Object(fallback_class.as_str().to_string()),
        Op::DynamicObjectNew.default_effects(),
        Some(expr.span),
    )
}

/// Returns constructor signature metadata when available for a fixed class.
pub(super) fn constructor_signature<'a>(
    ctx: &'a LoweringContext<'_, '_>,
    class_name: &Name,
) -> Option<&'a FunctionSig> {
    let key = php_symbol_key("__construct");
    ctx.classes
        .get(class_name.as_str().trim_start_matches('\\'))
        .and_then(|class_info| class_info.methods.get(&key))
}
