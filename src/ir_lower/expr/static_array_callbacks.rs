//! Purpose:
//! Static array callback lowering for map and walk.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers `array_map()` for a static callback and indexed array literal source.
pub(super) fn lower_static_array_map(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "array_map" || args.len() != 2 {
        return None;
    }
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return None;
    }
    if matches!(args[0].kind, ExprKind::Variable(_)) {
        return None;
    }
    let callback = static_call_user_func_callback(ctx, &args[0])?;
    let ExprKind::ArrayLiteral(items) = &args[1].kind else {
        return None;
    };
    // This fast path calls the callback once per element, interleaved with that element's own
    // expression. PHP evaluates the whole source array first, so only elements whose evaluation
    // cannot be observed may be reordered around a call.
    if !items.iter().all(static_callback_array_item_can_inline) {
        return None;
    }
    // Decide directly-callable-ness before emitting anything: a refusal discovered between two
    // elements would abandon a half-built result the caller's fallback cannot see.
    if !static_callable_call_lowers_directly(ctx, &callback) {
        return None;
    }
    let elem_type = static_callable_return_type(ctx, &callback);

    let array_type = PhpType::Array(Box::new(elem_type.clone()));

    // A callback that returns nothing has no element representation, and this path builds the
    // result array from the callback's OWN return type rather than from the checker's answer.
    // `array_map(nop(...), [1, 2])` is `[null, null]` in php-src; here it reached `array_push`
    // with a `Void` operand, which has no lowering, so a program php accepts was refused.
    // Declining hands it to the ordinary runtime path, which boxes each result — the same answer
    // the closure-literal form already gave, since only this static shape took the shortcut.
    if matches!(elem_type.codegen_repr(), PhpType::Void) {
        return None;
    }

    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(items.len() as u32)),
        array_type.clone(),
        Op::ArrayNew.default_effects(),
        Some(expr.span),
    );
    // The partially built result owns every mapped element already in it, and each callback can
    // throw, so publish it for the whole construction and reload it after every push: a push may
    // grow the payload and store a new pointer into the slot.
    let owner = publish_constructed_container(ctx, array, expr.span);
    for item in items {
        let value = lower_static_callable_call(ctx, callback.clone(), std::slice::from_ref(item), expr)
            .expect("a directly callable static binding lowers each mapped element");
        let array = load_published_container(ctx, owner, array_type.clone(), item.span);
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(item.span),
        );
        crate::ir_lower::stmt::release_indexed_array_write_operand(ctx, Some(&elem_type), value, item.span);
    }
    Some(take_published_container(ctx, owner, array_type, expr.span))
}

/// Lowers `array_walk()` for a static callback and immediate indexed-array literal.
pub(super) fn lower_static_array_walk(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "array_walk" || args.len() != 2 {
        return None;
    }
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return None;
    }
    if matches!(args[1].kind, ExprKind::Variable(_)) {
        return None;
    }
    let ExprKind::ArrayLiteral(items) = &args[0].kind else {
        return None;
    };
    if !items.iter().all(static_callback_array_item_can_inline) {
        return None;
    }
    let callback = static_call_user_func_callback(ctx, &args[1])?;
    // A receiver-bound callable has no value-operand call form. Refuse it before the first
    // element is lowered, rather than abandoning emitted instructions partway through.
    if matches!(callback, StaticCallableBinding::InstanceMethod { .. }) {
        return None;
    }
    for item in items {
        let item_value = lower_expr(ctx, item);
        lower_static_callable_value_call(ctx, callback.clone(), vec![item_value.value], expr)
            .expect("a value-callable static binding lowers each walked element");
    }
    Some(lower_null(ctx, expr))
}

/// Returns whether a literal array element can be reordered around callback invocation safely.
pub(super) fn static_callback_array_item_can_inline(item: &Expr) -> bool {
    matches!(
        item.kind,
        ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::Null
    )
}

/// Returns the best known element type for a static callback used by `array_map()`.
pub(super) fn static_callable_return_type(
    ctx: &LoweringContext<'_, '_>,
    target: &StaticCallableBinding,
) -> PhpType {
    match target {
        StaticCallableBinding::UserFunction(name)
        | StaticCallableBinding::ExternFunction(name)
        | StaticCallableBinding::Builtin(name) => call_return_type(ctx, name, &[]),
        StaticCallableBinding::Closure { signature, .. } => {
            normalize_value_php_type(crate::types::eir_value_php_type(&signature.return_type))
        }
        StaticCallableBinding::StaticMethod { receiver, method }
        | StaticCallableBinding::StaticMethodDescriptor { receiver, method } => {
            static_method_implementation_signature(ctx, receiver, method)
                .map(|signature| normalize_value_php_type(crate::types::eir_value_php_type(&signature.return_type)))
                .unwrap_or(PhpType::Mixed)
        }
        StaticCallableBinding::InstanceMethod { signature, .. } => {
            normalize_value_php_type(crate::types::eir_value_php_type(&signature.return_type))
        }
    }
}

/// Lowers one resolved static callable target using already-evaluated positional operands.
pub(super) fn lower_static_callable_value_call(
    ctx: &mut LoweringContext<'_, '_>,
    target: StaticCallableBinding,
    operands: Vec<crate::ir::ValueId>,
    expr: &Expr,
) -> Option<LoweredValue> {
    match target {
        StaticCallableBinding::UserFunction(function_name) => {
            let php_type = call_return_type(ctx, &function_name, &operands);
            let sig = ctx.functions.get(&function_name).cloned();
            let data = ctx.intern_function_name(&function_name);
            let call = ctx.emit_value(
                Op::Call,
                operands.clone(),
                Some(Immediate::Data(data)),
                php_type,
                effects_lookup::user_call_effects(&function_name),
                Some(expr.span),
            );
            ctx.invalidate_callable_user_function(&function_name, sig.as_ref(), &operands);
            Some(call)
        }
        StaticCallableBinding::ExternFunction(function_name) => {
            let php_type = call_return_type(ctx, &function_name, &operands);
            let data = ctx.intern_function_name(&function_name);
            Some(ctx.emit_value(
                Op::ExternCall,
                operands,
                Some(Immediate::Data(data)),
                php_type,
                Op::ExternCall.default_effects(),
                Some(expr.span),
            ))
        }
        StaticCallableBinding::Builtin(function_name) => {
            let sig = call_signature(
                ctx,
                &function_name,
                source_prefers_extension_builtin(&function_name),
            );
            let mut operands = operands;
            box_nullable_pointer_builtin_operands(ctx, sig.as_ref(), &mut operands, expr.span);
            let php_type = static_callable_builtin_result_type(
                ctx,
                &function_name,
                &operands,
                expr.span,
            );
            Some(emit_builtin_call_value(
                ctx,
                &function_name,
                operands,
                php_type,
                expr.span,
                None,
            ))
        }
        StaticCallableBinding::Closure {
            name,
            signature,
            captures,
            return_alias: _,
            ..
        } => {
            let arguments = operands;
            let mut call_operands = arguments.clone();
            append_closure_capture_operands(&mut call_operands, &captures);
            let php_type = normalize_value_php_type(crate::types::eir_value_php_type(&signature.return_type));
            let data = ctx.intern_function_name(&name);
            let call = ctx.emit_value(
                Op::Call,
                call_operands,
                Some(Immediate::Data(data)),
                php_type,
                effects_lookup::user_call_effects(&name),
                Some(expr.span),
            );
            ctx.invalidate_callable_capture_locals(&captures);
            ctx.invalidate_callable_ref_argument_locals(Some(&signature), &arguments);
            Some(call)
        }
        StaticCallableBinding::StaticMethod { receiver, method } => {
            let sig = static_method_implementation_signature(ctx, &receiver, &method).cloned();
            let result_type = sig
                .as_ref()
                .map(|signature| normalize_value_php_type(crate::types::eir_value_php_type(&signature.return_type)))
                .unwrap_or_else(|| fallback_expr_type(expr));
            let name = format!("{}::{}", receiver_name(&receiver), method);
            let data = ctx.intern_string(&name);
            let call = ctx.emit_value(
                Op::StaticMethodCall,
                operands.clone(),
                Some(Immediate::Data(data)),
                result_type,
                Op::StaticMethodCall.default_effects(),
                Some(expr.span),
            );
            ctx.invalidate_callable_ref_argument_locals(sig.as_ref(), &operands);
            Some(call)
        }
        StaticCallableBinding::StaticMethodDescriptor { receiver, method } => {
            lower_static_method_descriptor_value_call(ctx, &receiver, &method, operands, expr)
        }
        StaticCallableBinding::InstanceMethod { .. } => None,
    }
}

/// Resolves a compile-time `call_user_func*` callback expression.
pub(super) fn static_call_user_func_callback(
    ctx: &LoweringContext<'_, '_>,
    callback: &Expr,
) -> Option<StaticCallableBinding> {
    match &callback.kind {
        ExprKind::StringLiteral(name) => resolve_static_string_callable(ctx, name),
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
            resolve_static_string_callable(ctx, name.as_str())
        }
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            resolve_static_method_callable(ctx, receiver.clone(), method.clone())
        }
        ExprKind::Variable(name) => ctx
            .static_callable_local(name)
            .and_then(direct_static_callable_binding),
        ExprKind::ArrayLiteral(items) => static_array_callable_descriptor_target(ctx, items)
            .or_else(|| instance_array_callable_target(ctx, items)),
        _ => None,
    }
}

/// Resolves `call_user_func*` callbacks that must keep descriptor receiver state.
pub(super) fn instance_call_user_func_callback(
    ctx: &LoweringContext<'_, '_>,
    callback: &Expr,
) -> Option<StaticCallableBinding> {
    let target = match &callback.kind {
        ExprKind::FirstClassCallable(CallableTarget::Method { .. }) => {
            static_callable_binding_for_expr(ctx, callback)
        }
        ExprKind::Variable(name) => ctx.static_callable_local(name),
        _ => None,
    }?;
    if matches!(target, StaticCallableBinding::InstanceMethod { .. }) {
        Some(target)
    } else {
        None
    }
}

/// Returns signature metadata for receiver-bound callables that still need descriptor state.
pub(super) fn instance_callable_signature(target: &StaticCallableBinding) -> Option<&FunctionSig> {
    match target {
        StaticCallableBinding::InstanceMethod { signature, .. } => Some(signature),
        _ => None,
    }
}

/// Lowers `array_reduce($ints, fn($carry, $item) => $carry + $item, 0)` to the typed int sum.
///
/// The closure is an untyped pure addition of its two parameters and the initial value is the
/// additive identity, so PHP's left-to-right reduction and `array_sum()` perform the same
/// integer additions with the same overflow-to-float promotion. A declared return type, a
/// capture, a different initial value, or any other body keeps the generic reduce path.
pub(super) fn lower_reduce_sum(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "array_reduce" || args.len() != 3 {
        return None;
    }
    if ctx.functions.contains_key(name.trim_start_matches('\\'))
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    let ExprKind::Variable(source_name) = &args[0].kind else {
        return None;
    };
    if ctx.local_type(source_name).codegen_repr()
        != PhpType::Array(Box::new(PhpType::Int))
    {
        return None;
    }
    if !matches!(args[2].kind, ExprKind::IntLiteral(0)) {
        return None;
    }
    let ExprKind::Closure {
        params,
        variadic: None,
        return_type: None,
        body,
        captures,
        capture_refs,
        ..
    } = &args[1].kind
    else {
        return None;
    };
    if params.len() != 2
        || params
            .iter()
            .any(|(_, type_ann, _, is_ref)| type_ann.is_some() || *is_ref)
        || !captures.is_empty()
        || !capture_refs.is_empty()
    {
        return None;
    }
    let [Stmt { kind: StmtKind::Return(Some(body)), .. }] = body.as_slice() else {
        return None;
    };
    let ExprKind::BinaryOp { left, op: BinOp::Add, right } = &body.kind else {
        return None;
    };
    let is_param = |expr: &Expr, index: usize| {
        matches!(&expr.kind, ExprKind::Variable(name) if name == &params[index].0)
    };
    if !((is_param(left, 0) && is_param(right, 1))
        || (is_param(left, 1) && is_param(right, 0)))
    {
        return None;
    }
    let source = lower_expr(ctx, &args[0]);
    Some(ctx.emit_value(
        Op::RuntimeCall,
        vec![source.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(
            crate::ir::RuntimeFnId::ArraySum,
        ))),
        PhpType::Mixed,
        crate::ir::RuntimeFnId::ArraySum.intrinsic_effects(),
        Some(expr.span),
    ))
}

/// Lowers `count(explode($separator, $subject))` to `substr_count($subject, $separator) + 1`.
///
/// The exploded array is only observed through its element count, and a non-empty literal
/// separator makes `explode()` infallible there. Counting non-overlapping separator occurrences
/// produces exactly the same segment count without building or boxing any segment strings.
pub(super) fn lower_count_explode(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "count" || args.len() != 1 {
        return None;
    }
    if ctx.functions.contains_key(name.trim_start_matches('\\'))
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    let ExprKind::FunctionCall {
        name: explode_name,
        args: explode_args,
    } = &args[0].kind
    else {
        return None;
    };
    if php_symbol_key(explode_name.as_str().trim_start_matches('\\')) != "explode"
        || ctx.functions.contains_key(explode_name.as_str().trim_start_matches('\\'))
        || explode_args.len() != 2
        || crate::types::call_args::has_named_args(explode_args)
        || explode_args.iter().any(is_spread_arg)
    {
        return None;
    }
    let ExprKind::StringLiteral(separator) = &explode_args[0].kind else {
        return None;
    };
    if separator.is_empty() {
        return None;
    }
    let subject = lower_expr(ctx, &explode_args[1]);
    let data = ctx.intern_string(separator);
    let needle = ctx.emit_value(
        Op::ConstStr,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Str,
        Op::ConstStr.default_effects(),
        Some(expr.span),
    );
    let count = ctx.emit_value(
        Op::RuntimeCall,
        vec![subject.value, needle.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(
            crate::ir::RuntimeFnId::SubstrCount,
        ))),
        PhpType::Int,
        crate::ir::RuntimeFnId::SubstrCount.intrinsic_effects(),
        Some(expr.span),
    );
    let one = ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(1)),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(expr.span),
    );
    Some(ctx.emit_value(
        Op::IAdd,
        vec![count.value, one.value],
        None,
        PhpType::Int,
        Op::IAdd.default_effects(),
        Some(expr.span),
    ))
}

/// Lowers `array_sum(array_map(fn($x) => $x + $c, $ints))` into one packed-int kernel.
///
/// The closure has no captures, no declared return type, and a single untyped parameter, so
/// every element is an integer and the map result is an int except on PHP's int-to-float
/// overflow. The kernel interleaves map and sum in source order: an overflowing mapped value
/// promotes the running sum exactly where `array_map` followed by `array_sum` would.
pub(super) fn lower_map_sum_add(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "array_sum" || args.len() != 1 {
        return None;
    }
    if ctx.functions.contains_key(name.trim_start_matches('\\'))
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    let ExprKind::FunctionCall {
        name: map_name,
        args: map_args,
    } = &args[0].kind
    else {
        return None;
    };
    if php_symbol_key(map_name.as_str().trim_start_matches('\\')) != "array_map"
        || ctx.functions.contains_key(map_name.as_str().trim_start_matches('\\'))
        || map_args.len() != 2
        || crate::types::call_args::has_named_args(map_args)
        || map_args.iter().any(is_spread_arg)
    {
        return None;
    }
    let ExprKind::Variable(source_name) = &map_args[1].kind else {
        return None;
    };
    if ctx.local_type(source_name).codegen_repr()
        != PhpType::Array(Box::new(PhpType::Int))
    {
        return None;
    }
    let (param, body) = scalar_single_param_closure(&map_args[0], false)?;
    let offset = match &body.kind {
        ExprKind::BinaryOp { left, op: BinOp::Add, right } => {
            additive_literal(left, right, param)?
        }
        _ => return None,
    };
    let source = lower_expr(ctx, &map_args[1]);
    let offset = ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(offset)),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(expr.span),
    );
    Some(ctx.emit_value(
        Op::RuntimeCall,
        vec![source.value, offset.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(
            crate::ir::RuntimeFnId::ArrayMapSumAdd,
        ))),
        PhpType::Mixed,
        crate::ir::RuntimeFnId::ArrayMapSumAdd.intrinsic_effects(),
        Some(expr.span),
    ))
}

/// Lowers `count(array_filter($ints, fn($x) => $x % $c === 0))` into one counting kernel.
///
/// The predicate is exactly PHP's integer remainder compared strictly with zero, so the kernel
/// only has to test divisibility. A zero divisor is left to the generic path because PHP would
/// throw `DivisionByZeroError`; every other literal divisor keeps the same truth table.
pub(super) fn lower_filter_count_mod(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "count" || args.len() != 1 {
        return None;
    }
    if ctx.functions.contains_key(name.trim_start_matches('\\'))
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    let ExprKind::FunctionCall {
        name: filter_name,
        args: filter_args,
    } = &args[0].kind
    else {
        return None;
    };
    if php_symbol_key(filter_name.as_str().trim_start_matches('\\')) != "array_filter"
        || ctx.functions.contains_key(filter_name.as_str().trim_start_matches('\\'))
        || filter_args.len() != 2
        || crate::types::call_args::has_named_args(filter_args)
        || filter_args.iter().any(is_spread_arg)
    {
        return None;
    }
    let ExprKind::Variable(source_name) = &filter_args[0].kind else {
        return None;
    };
    if ctx.local_type(source_name).codegen_repr()
        != PhpType::Array(Box::new(PhpType::Int))
    {
        return None;
    }
    let (param, body) = scalar_single_param_closure(&filter_args[1], true)?;
    let ExprKind::BinaryOp { left, op: BinOp::StrictEq, right } = &body.kind else {
        return None;
    };
    if !matches!(right.kind, ExprKind::IntLiteral(0)) {
        return None;
    }
    let ExprKind::BinaryOp { left: value, op: BinOp::Mod, right: divisor } = &left.kind else {
        return None;
    };
    if !matches!(&value.kind, ExprKind::Variable(name) if name == param) {
        return None;
    }
    let ExprKind::IntLiteral(divisor) = divisor.kind else {
        return None;
    };
    if divisor == 0 {
        return None;
    }
    let source = lower_expr(ctx, &filter_args[0]);
    let divisor = ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(divisor)),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(expr.span),
    );
    Some(ctx.emit_value(
        Op::RuntimeCall,
        vec![source.value, divisor.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(
            crate::ir::RuntimeFnId::ArrayFilterCountMod,
        ))),
        PhpType::Int,
        crate::ir::RuntimeFnId::ArrayFilterCountMod.intrinsic_effects(),
        Some(expr.span),
    ))
}

/// Returns the parameter name and single return expression of a capture-free scalar closure.
///
/// The parameter may be declared `int` because a packed int source passes integers unchanged.
/// The return type must stay absent for the map kernel, whose overflowing result becomes a float
/// that a declared `int` return would reject with a `TypeError`; the filter kernel may accept a
/// declared `bool` return because a strict comparison always produces a bool.
fn scalar_single_param_closure(expr: &Expr, allow_bool_return: bool) -> Option<(&str, &Expr)> {
    let ExprKind::Closure {
        params,
        variadic: None,
        return_type,
        body,
        captures,
        capture_refs,
        ..
    } = &expr.kind
    else {
        return None;
    };
    let [(param, param_type, default, is_ref)] = params.as_slice() else {
        return None;
    };
    if !matches!(param_type, None | Some(TypeExpr::Int))
        || default.is_some()
        || *is_ref
        || !captures.is_empty()
        || !capture_refs.is_empty()
    {
        return None;
    }
    let declared_return_ok = match return_type {
        None => true,
        Some(TypeExpr::Bool) => allow_bool_return,
        Some(_) => false,
    };
    if !declared_return_ok {
        return None;
    }
    let [Stmt { kind: StmtKind::Return(Some(body)), .. }] = body.as_slice() else {
        return None;
    };
    Some((param.as_str(), body))
}

/// Extracts `$param + $literal` or `$literal + $param` as the literal addend.
fn additive_literal(left: &Expr, right: &Expr, param: &str) -> Option<i64> {
    match (&left.kind, &right.kind) {
        (ExprKind::Variable(name), ExprKind::IntLiteral(value)) if name == param => Some(*value),
        (ExprKind::IntLiteral(value), ExprKind::Variable(name)) if name == param => Some(*value),
        _ => None,
    }
}

/// Lowers `is_int($a[$i])` and its sibling predicates into one in-place tag read.
///
/// The receiver must be a packed array of boxed Mixed cells and the index a statically-int
/// expression. The kernel compares the stored runtime tag against a mask, so it never
/// allocates or releases a Mixed cell; hashes, references, nested wrappers and missing keys
/// fall back to the generic read, warning and ownership included.
pub(super) fn lower_array_element_type_check(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let canonical = php_symbol_key(name.trim_start_matches('\\'));
    let mask = element_type_predicate_mask(&canonical)?;
    if ctx.functions.contains_key(name.trim_start_matches('\\'))
        || args.len() != 1
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    let ExprKind::ArrayAccess { array, index } = &args[0].kind else {
        return None;
    };
    let ExprKind::Variable(source_name) = &array.kind else {
        return None;
    };
    if !matches!(
        ctx.local_type(source_name).codegen_repr(),
        PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed
    ) || !statically_int_expr(ctx, index)
    {
        return None;
    }
    let array = lower_expr(ctx, array);
    let index = lower_expr(ctx, index);
    let mask = ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(mask)),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(expr.span),
    );
    Some(ctx.emit_value(
        Op::RuntimeCall,
        vec![array.value, index.value, mask.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(
            crate::ir::RuntimeFnId::ArrayElementTypeCheck,
        ))),
        PhpType::Bool,
        crate::ir::RuntimeFnId::ArrayElementTypeCheck.intrinsic_effects(),
        Some(expr.span),
    ))
}

/// Returns the runtime-tag mask for one fused element predicate.
fn element_type_predicate_mask(name: &str) -> Option<i64> {
    Some(match name {
        "is_int" => 1 << 0,
        "is_string" => 1 << 1,
        "is_float" => 1 << 2,
        "is_bool" => 1 << 3,
        "is_array" => (1 << 4) | (1 << 5),
        "is_null" => 1 << 8,
        "is_scalar" => (1 << 0) | (1 << 1) | (1 << 2) | (1 << 3),
        _ => return None,
    })
}

/// Returns whether `expr` is statically an integer without lowering it or reading EIR types.
fn statically_int_expr(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::IntLiteral(_) => true,
        ExprKind::Variable(name) => ctx.local_type(name).codegen_repr() == PhpType::Int,
        ExprKind::Negate(inner) | ExprKind::BitNot(inner) => statically_int_expr(ctx, inner),
        ExprKind::Cast { target: CastType::Int, .. } => true,
        ExprKind::BinaryOp { left, op, right } => {
            matches!(
                op,
                BinOp::Add
                    | BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Mod
                    | BinOp::BitAnd
                    | BinOp::BitOr
                    | BinOp::BitXor
                    | BinOp::ShiftLeft
                    | BinOp::ShiftRight
            ) && statically_int_expr(ctx, left)
                && statically_int_expr(ctx, right)
        }
        _ => false,
    }
}

/// Lowers a declared-int `array_sum(array_map(fn(int $x): int => $x + C, $ints))`.
///
/// A declared `int` return rejects an overflow-promoted map result with PHP's catchable
/// `TypeError`, so the fused kernel is only valid when no element overflows. The preflight
/// scans the packed payload once; a safe range takes the fused kernel, while an unsafe range
/// falls back to the ordinary typed `array_map()` + `array_sum()` lowering, preserving the
/// exact TypeError at the first overflowing element.
pub(super) fn lower_map_sum_add_declared_int(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "array_sum" || args.len() != 1 {
        return None;
    }
    if ctx.functions.contains_key(name.trim_start_matches('\\'))
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    let ExprKind::FunctionCall {
        name: map_name,
        args: map_args,
    } = &args[0].kind
    else {
        return None;
    };
    if php_symbol_key(map_name.as_str().trim_start_matches('\\')) != "array_map"
        || ctx.functions.contains_key(map_name.as_str().trim_start_matches('\\'))
        || map_args.len() != 2
        || crate::types::call_args::has_named_args(map_args)
        || map_args.iter().any(is_spread_arg)
    {
        return None;
    }
    let ExprKind::Variable(source_name) = &map_args[1].kind else {
        return None;
    };
    if ctx.local_type(source_name).codegen_repr()
        != PhpType::Array(Box::new(PhpType::Int))
    {
        return None;
    }
    let (param, body) = declared_int_single_param_closure(&map_args[0])?;
    let offset = match &body.kind {
        ExprKind::BinaryOp { left, op: BinOp::Add, right } => {
            additive_literal(left, right, param)?
        }
        _ => return None,
    };
    let source = lower_expr(ctx, &map_args[1]);
    let offset = ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(offset)),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(expr.span),
    );
    let preflight_effects = crate::ir::RuntimeFnId::ArrayMapSumAddSafe.intrinsic_effects();
    let safe = ctx.emit_value(
        Op::RuntimeCall,
        vec![source.value, offset.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(
            crate::ir::RuntimeFnId::ArrayMapSumAddSafe,
        ))),
        PhpType::Bool,
        preflight_effects,
        Some(expr.span),
    );
    let fused = ctx.builder.create_named_block("map_sum.fused", Vec::new());
    let generic = ctx.builder.create_named_block("map_sum.generic", Vec::new());
    let merge = ctx.builder.create_named_block("map_sum.merge", Vec::new());
    let temp_name = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let split_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(crate::ir::Terminator::CondBr {
        cond: safe.value,
        then_target: fused,
        then_args: Vec::new(),
        else_target: generic,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(fused);
    ctx.restore_initialized_slots(split_initialized.clone());
    let fused_value = ctx.emit_value(
        Op::RuntimeCall,
        vec![source.value, offset.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(
            crate::ir::RuntimeFnId::ArrayMapSumAdd,
        ))),
        PhpType::Mixed,
        crate::ir::RuntimeFnId::ArrayMapSumAdd.intrinsic_effects(),
        Some(expr.span),
    );
    super::merge_temps::store_value_into_temp(
        ctx,
        &temp_name,
        PhpType::Mixed,
        fused_value,
        expr.span,
    );
    let fused_reachable = !ctx.builder.insertion_block_is_terminated();
    let fused_initialized = ctx.initialized_slots_snapshot();
    super::merge_temps::branch_to(ctx, merge);

    ctx.builder.position_at_end(generic);
    ctx.restore_initialized_slots(split_initialized.clone());
    let mapped = lower_expr(ctx, &args[0]);
    let summed = super::function_calls::emit_builtin_call_value(
        ctx,
        "array_sum",
        vec![mapped.value],
        PhpType::Mixed,
        expr.span,
        None,
    );
    super::merge_temps::store_value_into_temp(
        ctx,
        &temp_name,
        PhpType::Mixed,
        summed,
        expr.span,
    );
    let generic_reachable = !ctx.builder.insertion_block_is_terminated();
    let generic_initialized = ctx.initialized_slots_snapshot();
    super::merge_temps::branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(super::merge_temps::merge_initialized_slots_for_expr(
        &split_initialized,
        fused_initialized,
        fused_reachable,
        generic_initialized,
        generic_reachable,
    ));
    Some(super::merge_temps::take_owned_temp(ctx, &temp_name, expr.span))
}

/// Returns the parameter name and return expression of a capture-free `: int` scalar closure.
fn declared_int_single_param_closure(expr: &Expr) -> Option<(&str, &Expr)> {
    let ExprKind::Closure {
        params,
        variadic: None,
        return_type: Some(TypeExpr::Int),
        body,
        captures,
        capture_refs,
        ..
    } = &expr.kind
    else {
        return None;
    };
    let [(param, param_type, default, is_ref)] = params.as_slice() else {
        return None;
    };
    if !matches!(param_type, None | Some(TypeExpr::Int))
        || default.is_some()
        || *is_ref
        || !captures.is_empty()
        || !capture_refs.is_empty()
    {
        return None;
    }
    let [Stmt { kind: StmtKind::Return(Some(body)), .. }] = body.as_slice() else {
        return None;
    };
    Some((param.as_str(), body))
}
