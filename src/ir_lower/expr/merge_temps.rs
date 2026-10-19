//! Purpose:
//! Branch merge temporaries, container widening, and fallback typing.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Stores a lowered expression result into a hidden merge temporary.
pub(super) fn store_expr_into_temp(
    ctx: &mut LoweringContext<'_, '_>,
    temp_name: &str,
    temp_type: PhpType,
    expr: &Expr,
    span: crate::span::Span,
) {
    let value = lower_expr(ctx, expr);
    store_value_into_temp(ctx, temp_name, temp_type, value, span);
}

/// Stores an already lowered value into a hidden merge temporary.
pub(super) fn store_value_into_temp(
    ctx: &mut LoweringContext<'_, '_>,
    temp_name: &str,
    temp_type: PhpType,
    value: LoweredValue,
    span: crate::span::Span,
) {
    let value = coerce_value_for_temp(ctx, value, &temp_type, span);
    let source = value;
    let stored = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span));
    ctx.store_local(temp_name, stored, temp_type, Some(span));
    if stored.value != source.value && ctx.value_needs_release_after_retaining_store(source) {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    }
}

/// Loads an owned hidden temp into SSA and clears the backing slot without releasing it.
pub(super) fn take_owned_temp(
    ctx: &mut LoweringContext<'_, '_>,
    temp_name: &str,
    span: crate::span::Span,
) -> LoweredValue {
    let value = ctx.load_local(temp_name, Some(span));
    ctx.clear_owned_hidden_temp(temp_name, Some(span));
    value
}

/// One lazily lowered branch result of a ternary, `?:`, `??` or `match`, held in SSA until every
/// branch of the merge is lowered.
///
/// The merge temporary used to be declared BEFORE its branches were lowered, with a type guessed
/// from the branch expressions (`materialized_expr_type_for_merge`). Any expression the guess
/// could not name fell back to the syntactic `int` default, and the branch value was then coerced
/// into that slot: `$c ? null : $r->n` over a `mixed` `$r` printed the string payload's pointer as
/// an integer, and `$r === false ? false : $r->n` printed `int(0)` for a string property. Parking
/// each branch value and storing it only once every branch's real lowered type is known makes the
/// merge type a fact rather than a guess.
pub(super) struct PendingBranchValue {
    /// The block the branch value was produced in; it is still open (unterminated).
    block: BlockId,
    value: LoweredValue,
    /// The type this branch contributes to the merge (normally the value's own type); `None` when
    /// the branch can never yield a value at run time and so constrains nothing.
    merge_type: Option<PhpType>,
    /// Definitely initialized slots at the end of the branch.
    initialized: HashSet<LocalSlotId>,
    /// A tested-but-not-forwarded value (`?:`/`??` default arm) released after the store.
    discarded: Option<LoweredValue>,
    /// Flow-sensitive local types at the end of the branch, restored before its store when a
    /// [`crate::ir_lower::stmt::LazyBranchJoin`] reconciles the edges (see `store_pending_branches`).
    types: crate::types::TypeEnv,
}

impl PendingBranchValue {
    /// Drops the null member from the type this branch contributes to the merge, for an arm a
    /// guard has proven non-null (the value arm of `??`, the truthy arm of `?:`). An arm whose
    /// only possible value is null is unreachable and contributes nothing.
    pub(super) fn excluding_null(mut self) -> Self {
        self.merge_type = match self.merge_type.take().map(non_null_merge_type) {
            Some(PhpType::Void) | None => None,
            Some(non_null) => Some(normalize_value_php_type(non_null)),
        };
        self
    }

    /// Returns the definitely initialized slots at the end of the branch, after its store.
    pub(super) fn initialized(&self) -> &HashSet<LocalSlotId> {
        &self.initialized
    }
}

/// Returns the type a proven-non-null arm of `??`/`?:` contributes to a merge: `php_type` without
/// its null member, when the merge store can reach that narrower representation from the value's
/// own one.
///
/// A scalar or array narrowed out of a boxed `mixed` cell is converted by `coerce_value_for_temp`
/// (cast or payload unbox), and a tagged `?int` narrows to its raw scalar. A nullable object
/// (`?X`, represented as a boxed `mixed` cell) has no such conversion: narrowing it to `X` typed
/// the merge temp as an object pointer and stored the BOX pointer in it, so `$p ?: $d` over
/// `?X $p` read garbage or crashed. Such an arm keeps its nullable type, and the merge stays boxed.
pub(super) fn non_null_merge_type(php_type: PhpType) -> PhpType {
    let value_repr = php_type.codegen_repr();
    let non_null = strip_void_from_union(php_type.clone());
    let narrowed_repr = non_null.codegen_repr();
    let reachable = narrowed_repr == value_repr
        || value_repr != PhpType::Mixed
        || matches!(
            narrowed_repr,
            PhpType::Void
                | PhpType::Int
                | PhpType::Bool
                | PhpType::Float
                | PhpType::Str
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
        );
    if reachable {
        non_null
    } else {
        php_type
    }
}

/// Lowers one branch expression and parks its value; `None` when the branch cannot fall through.
pub(super) fn lower_pending_branch(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<PendingBranchValue> {
    let value = lower_expr(ctx, expr);
    park_pending_branch(ctx, value, None)
}

/// Parks an already lowered branch value at the current insertion point.
pub(super) fn park_pending_branch(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    discarded: Option<LoweredValue>,
) -> Option<PendingBranchValue> {
    if ctx.builder.insertion_block_is_terminated() {
        return None;
    }
    let block = ctx.builder.insertion_block()?;
    let merge_type = Some(normalize_value_php_type(ctx.builder.value_php_type(value.value)));
    Some(PendingBranchValue {
        block,
        value,
        merge_type,
        initialized: ctx.initialized_slots_snapshot(),
        discarded,
        types: ctx.local_types_snapshot(),
    })
}

/// Chooses the merge temp type from the branches' LOWERED value types.
///
/// Uses the same join as the pre-lowering estimate (`nullable_aware_branch_merge_type`), so
/// homogeneous branches keep their native representation (`int`/`int` stays an unboxed `int`,
/// `string`/`string` a string, one class an object pointer); only branches whose real
/// representations differ widen, to boxed `mixed` or a nullable tagged scalar.
pub(super) fn pending_branch_merge_type(branches: &[PendingBranchValue]) -> PhpType {
    let mut merged: Option<PhpType> = None;
    for branch in branches {
        let Some(ty) = &branch.merge_type else {
            continue;
        };
        merged = Some(match merged {
            Some(acc) => nullable_aware_branch_merge_type(&acc, ty),
            None => ty.clone(),
        });
    }
    merged.unwrap_or(PhpType::Void)
}

/// Declares the merge temp, stores every parked branch value into it at the end of its own block
/// and branches each one to `merge`. Returns the temp name and the parked branches, whose
/// `initialized` sets now include the temp store.
///
/// `container_fallback` is the whole merge expression for a ternary or `match`. A merge of
/// ARRAY branches keeps the widening against that expression's syntactic type that those
/// constructs always applied: a branch array can be a missed-read sentinel (`$rows[5]` over
/// `array<array<int>>`), which the boxed merge turns into PHP `null` but an elementwise
/// `ArrayToMixed` conversion would dereference. Scalar and object merges need no such guard and
/// keep their exact lowered type.
///
/// With a `join`, each branch's own local types are restored before its store and the branch
/// ends in the join's deferred tail instead of a direct branch to `merge`, so the join can
/// reconcile locals a branch retyped AFTER the temp store; the caller then calls
/// `LazyBranchJoin::finish`.
pub(super) fn store_pending_branches(
    ctx: &mut LoweringContext<'_, '_>,
    mut branches: Vec<PendingBranchValue>,
    merge: BlockId,
    container_fallback: Option<&Expr>,
    mut join: Option<&mut crate::ir_lower::stmt::LazyBranchJoin>,
    span: crate::span::Span,
) -> (String, Vec<PendingBranchValue>) {
    let mut result_type = pending_branch_merge_type(&branches);
    if let Some(whole) = container_fallback {
        if matches!(result_type.codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. }) {
            result_type = wider_type_for_merge(&fallback_expr_type(whole).codegen_repr(), &result_type);
        }
    }
    let temp_name = ctx.declare_owned_hidden_temp(result_type.clone());
    for branch in &mut branches {
        ctx.builder.position_at_end(branch.block);
        ctx.restore_initialized_slots(branch.initialized.clone());
        if join.is_some() {
            ctx.restore_local_types(branch.types.clone());
        }
        store_value_into_temp(ctx, &temp_name, result_type.clone(), branch.value, span);
        if let Some(discarded) = branch.discarded {
            release_discarded_branch_value(ctx, discarded, span);
        }
        branch.initialized = ctx.initialized_slots_snapshot();
        match join.as_deref_mut() {
            Some(join) => join.exit_edge(ctx),
            None => branch_to(ctx, merge),
        }
    }
    (temp_name, branches)
}

/// Estimates a ternary's merge type before it is lowered, for a consumer that must type a slot
/// first (an array-literal element, an enclosing merge estimate). The ternary itself types its
/// merge temp from the lowered branch values (see [`PendingBranchValue`]).
pub(super) fn branch_merge_result_type(
    ctx: &LoweringContext<'_, '_>,
    then_expr: &Expr,
    else_expr: &Expr,
    expr: &Expr,
) -> PhpType {
    let then_ty = materialized_expr_type_for_merge(ctx, then_expr);
    let else_ty = materialized_expr_type_for_merge(ctx, else_expr);
    let branch_ty = nullable_aware_branch_merge_type(&then_ty, &else_ty);
    if php_type_allows_null(&branch_ty) {
        return branch_ty;
    }
    let fallback_ty = fallback_expr_type(expr).codegen_repr();
    wider_type_for_merge(&fallback_ty, &branch_ty.codegen_repr())
}

/// Estimates a `match` merge type before it is lowered by merging every arm result estimate, so
/// heterogeneous arms (e.g. object/array/string) estimate a Mixed slot instead of one unified
/// scalar type. `lower_match` itself types its temp from the lowered arm values.
pub(super) fn match_merge_result_type(
    ctx: &LoweringContext<'_, '_>,
    arms: &[(Vec<Expr>, Expr)],
    default: Option<&Expr>,
    expr: &Expr,
) -> PhpType {
    let mut merged: Option<PhpType> = None;
    for result in arms.iter().map(|(_, result)| result).chain(default) {
        let arm_ty = materialized_expr_type_for_merge(ctx, result);
        merged = Some(match merged {
            Some(acc) => nullable_aware_branch_merge_type(&acc, &arm_ty),
            None => arm_ty,
        });
    }
    let Some(merged) = merged else {
        return fallback_expr_type(expr);
    };
    if php_type_allows_null(&merged) {
        return merged;
    }
    let fallback_ty = fallback_expr_type(expr).codegen_repr();
    wider_type_for_merge(&fallback_ty, &merged.codegen_repr())
}

/// Estimates a short-ternary merge type before it is lowered, without the scalar-biased
/// syntactic join; `lower_short_ternary` itself types its temp from the lowered arm values.
pub(super) fn short_ternary_merge_result_type(
    ctx: &LoweringContext<'_, '_>,
    value: &Expr,
    default: &Expr,
) -> PhpType {
    // Mirrors `lower_short_ternary`: the value arm only reaches the merge when truthy (so never
    // null), and a null default keeps the merge nullable.
    let value_ty = non_null_merge_type(materialized_expr_type_for_merge(ctx, value));
    let default_ty = materialized_expr_type_for_merge(ctx, default);
    nullable_aware_branch_merge_type(&value_ty, &default_ty)
}

/// Chooses a ternary branch merge type without erasing PHP null branches.
pub(super) fn nullable_aware_branch_merge_type(left: &PhpType, right: &PhpType) -> PhpType {
    if php_type_allows_null(left) || php_type_allows_null(right) {
        let left_non_null = strip_void_from_union(left.clone());
        let right_non_null = strip_void_from_union(right.clone());
        return normalize_union_members(vec![PhpType::Void, left_non_null, right_non_null])
            .unwrap_or(PhpType::Void);
    }
    wider_type_for_merge(&left.codegen_repr(), &right.codegen_repr())
}

/// Returns true when a PHP type can materialize PHP null at runtime.
pub(super) fn php_type_allows_null(php_type: &PhpType) -> bool {
    match php_type {
        PhpType::Void | PhpType::Never | PhpType::Mixed => true,
        PhpType::Union(members) => members
            .iter()
            .any(|member| matches!(member, PhpType::Void | PhpType::Never | PhpType::Mixed)),
        _ => false,
    }
}

/// Estimates the value type an expression will materialize, before it is lowered.
///
/// An expression it cannot name answers boxed `mixed` (never the syntactic `int` default), so a
/// slot typed from the estimate can always hold the lowered value.
pub(super) fn materialized_expr_type_for_merge(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> PhpType {
    match &expr.kind {
        ExprKind::Variable(name) => normalize_value_php_type(ctx.local_type(name).codegen_repr()),
        ExprKind::ErrorSuppress(inner) => materialized_expr_type_for_merge(ctx, inner),
        ExprKind::BinaryOp { left, op, right } if mixed_numeric_op(op).is_some() => {
            let left_ty = materialized_expr_type_for_merge(ctx, left).codegen_repr();
            let right_ty = materialized_expr_type_for_merge(ctx, right).codegen_repr();
            if matches!(left_ty, PhpType::Mixed | PhpType::Union(_))
                || matches!(right_ty, PhpType::Mixed | PhpType::Union(_))
            {
                PhpType::Mixed
            } else {
                fallback_expr_type(expr)
            }
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => branch_merge_result_type(ctx, then_expr, else_expr, expr),
        ExprKind::Match { arms, default, .. } => {
            match_merge_result_type(ctx, arms, default.as_deref(), expr)
        }
        ExprKind::ShortTernary { value, default } => {
            short_ternary_merge_result_type(ctx, value, default)
        }
        ExprKind::NullCoalesce { value, default } => {
            // Mirrors `lower_null_coalesce`: the value arm only reaches the merge when non-null.
            let value_ty = non_null_merge_type(materialized_expr_type_for_merge(ctx, value));
            let default_ty = materialized_expr_type_for_merge(ctx, default);
            nullable_aware_branch_merge_type(&value_ty, &default_ty)
        }
        // A receiver, container or constant this pre-lowering estimate cannot resolve says nothing
        // about the value's representation, and the syntactic fallback would answer `int` for it.
        // Every arm below names the type when it can and otherwise answers boxed `mixed`, which
        // holds whatever the expression yields.
        ExprKind::ArrayAccess { array, .. } => {
            array_access_expr_value_type_for_ir(ctx, array).unwrap_or(PhpType::Mixed)
        }
        ExprKind::PropertyAccess { object, property } => {
            property_access_expr_type_for_ir(ctx, object, property).unwrap_or(PhpType::Mixed)
        }
        ExprKind::NullsafePropertyAccess { object, property } => {
            nullsafe_property_access_expr_type_for_ir(ctx, object, property)
                .unwrap_or(PhpType::Mixed)
        }
        ExprKind::ConstRef(name) => ctx
            .constant_value(name.as_str())
            .map(|(_, ty)| normalize_value_php_type(ty))
            .unwrap_or(PhpType::Mixed),
        ExprKind::ScopedConstantAccess { receiver, name } => {
            scoped_constant_value_type_for_ir(ctx, receiver, name, expr)
        }
        ExprKind::Not(_) => PhpType::Bool,
        ExprKind::Clone(inner) => match materialized_expr_type_for_merge(ctx, inner).codegen_repr() {
            object @ PhpType::Object(_) => object,
            _ => PhpType::Mixed,
        },
        // Calls are typed from the return metadata their own lowering uses. The syntactic
        // fallback answers `int` for every call outside its builtin allowlist, and a merge temp
        // typed from that guess coerced a string, array or object branch into an integer slot or,
        // beside a `null` branch, into a tagged `?int` scalar the backend cannot store an array
        // in (issue #1364). A call whose result cannot be named here merges as boxed `mixed`,
        // which holds whatever the call returns.
        ExprKind::FunctionCall { name, .. } => function_call_type_for_merge(ctx, name.as_str(), expr),
        ExprKind::MethodCall { object, method, .. } => {
            method_call_expr_type_for_ir(ctx, object, method).unwrap_or(PhpType::Mixed)
        }
        ExprKind::NullsafeMethodCall { object, method, .. } => {
            nullsafe_method_call_expr_type_for_ir(ctx, object, method).unwrap_or(PhpType::Mixed)
        }
        ExprKind::StaticMethodCall { receiver, method, .. } => {
            static_method_call_expr_type_for_ir(ctx, receiver, method).unwrap_or(PhpType::Mixed)
        }
        ExprKind::ClosureCall { var, .. } => closure_call_type_for_merge(ctx, var),
        ExprKind::ExprCall { .. } => PhpType::Mixed,
        // An assignment yields the stored value. The syntactic fallback answers `int` for it, so
        // `$c ?: $o = new C` typed its merge temp as an integer and the backend refused to cast
        // the object. A plain local store yields its value's type; a write with a prelude or a
        // result temporary (a property, a dimension, an append) merges as boxed `mixed`.
        ExprKind::Assignment { target, value, result_target, prelude, .. } => {
            if result_target.is_none()
                && prelude.is_empty()
                && matches!(target.kind, ExprKind::Variable(_))
            {
                materialized_expr_type_for_merge(ctx, value)
            } else {
                PhpType::Mixed
            }
        }
        _ if syntactic_merge_type_is_exact(expr) => fallback_expr_type(expr),
        _ => PhpType::Mixed,
    }
}

/// Returns true when `infer_expr_type_syntactic` names the exact representation `expr` lowers to.
///
/// The syntactic inference ends in a `_ => int` default, so for any other expression kind (a
/// negation of a float, a static property, a dynamic property, a pipe, an assignment, ...) its
/// answer is a guess, and a merge temp typed from that guess coerced the branch into an integer
/// slot or a nullable tagged `?int`.
fn syntactic_merge_type_is_exact(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::Throw(_)
        | ExprKind::InstanceOf { .. }
        | ExprKind::NewObject { .. }
        | ExprKind::Closure { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::ArrayLiteral(_)
        | ExprKind::ArrayLiteralAssoc(_)
        | ExprKind::ArrayLiteralMixed(_)
        | ExprKind::Print(_) => true,
        ExprKind::Cast { target, .. } => matches!(
            target,
            CastType::Int | CastType::Float | CastType::String | CastType::Bool
        ),
        // Operator results keep the coercion order the operator lowering itself implements
        // (`.` is a string, comparisons are booleans, arithmetic is typed by its own rules).
        ExprKind::BinaryOp { .. } => true,
        _ => false,
    }
}

/// Returns the merge type of a named function call before it is lowered.
///
/// A user or extern function answers the return type `lower_function_call` stamps on the call.
/// A pop or shift uses its shared EIR result contract, which is boxed `Mixed` even when
/// the checker sees a scalar array element: removing from an empty array returns null.
/// Other builtins keep the syntactic answer when that names a type, and otherwise consult
/// the checker's result type for this very call (`int`, `bool`, `float` and `string` stay
/// scalar, anything else is boxed), because syntactic `int` is only a default outside its
/// allowlist.
fn function_call_type_for_merge(ctx: &LoweringContext<'_, '_>, canonical: &str, expr: &Expr) -> PhpType {
    if !source_prefers_extension_builtin(canonical) {
        if let Some(signature) = ctx.functions.get(canonical) {
            return normalize_value_php_type(eir_user_function_return_type(signature));
        }
    }
    if let Some(signature) = ctx.extern_functions.get(canonical) {
        return normalize_value_php_type(signature.return_type.clone());
    }
    if let Some(def) = crate::builtins::registry::lookup(canonical.trim_start_matches('\\')) {
        if matches!(
            def.spec.semantics.lowering,
            crate::builtins::semantics::BuiltinLowering::Runtime(
                crate::ir::RuntimeCallTarget::Function(
                    crate::ir::RuntimeFnId::ArrayPop | crate::ir::RuntimeFnId::ArrayShift
                )
            )
        ) {
            return resolve_registry_builtin_result_type(
                ctx,
                def.name,
                &[],
                &[],
                expr.span,
                None,
            )
            .expect("registered pop/shift builtin must resolve its result type");
        }
    }
    let syntactic = fallback_expr_type(expr);
    if syntactic != PhpType::Int {
        return syntactic;
    }
    builtin_call_result_type_for_ir(ctx, expr.span).unwrap_or(syntactic)
}

/// Returns the merge type of a `$callable(...)` call through a local variable.
///
/// Mirrors `lower_closure_call`: a statically known closure or user function answers its
/// signature's return type and a typed callable parameter its descriptor signature's.
/// Everything else (builtin names, methods, invokable objects, unknown callables) merges as
/// boxed `mixed`.
fn closure_call_type_for_merge(ctx: &LoweringContext<'_, '_>, var: &str) -> PhpType {
    match ctx.static_callable_local(var) {
        Some(target @ StaticCallableBinding::Closure { .. })
        | Some(target @ StaticCallableBinding::UserFunction(_)) => {
            return static_callable_return_type(ctx, &target);
        }
        Some(_) => return PhpType::Mixed,
        None => {}
    }
    ctx.callable_param_signature(var)
        .map(|signature| descriptor_invoker_result_type(Some(signature)))
        .unwrap_or(PhpType::Mixed)
}

/// Coerces branch values to the hidden temp storage type before storing them.
pub(super) fn coerce_value_for_temp(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    temp_type: &PhpType,
    span: crate::span::Span,
) -> LoweredValue {
    let target_ty = temp_type.codegen_repr();
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    if source_ty == target_ty {
        return value;
    }
    match &target_ty {
        PhpType::Mixed => ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span)),
        PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never => {
            coerce_to_int_at_span(ctx, value, Some(span))
        }
        PhpType::Float => coerce_to_float_at_span(ctx, value, Some(span)),
        PhpType::Str => coerce_to_string_at_span(ctx, value, Some(span)),
        // A boxed `?C` (a nullable parameter or property) merged into a `C` temp: `$o ??= new C()`
        // types the merge from the non-null side, so the arm that keeps `$o` must hand over the
        // object inside the cell. Storing the cell pointer itself made the next member access
        // read the box as the object (#1628). The unbox owns its own lease of the payload.
        // `MixedUnbox` does not check the tag, so this arm relies on never seeing null: the
        // only producer of an object temp from a nullable value is `null_coalesce_result_type`,
        // and its value arm runs behind `IsNull`. Ternary, `match` and `?:` merges keep a
        // nullable arm in a `Mixed` temp and never reach here. The default arms of
        // `??`/`??=` are safe the same way: their materialized type is `Object` only because
        // `materialized_expr_type_for_merge` mirrors the result type each lowering actually
        // stamps, so a default can never hand a null-bearing cell to this arm either — keep
        // that mirroring if either side changes.
        PhpType::Object(_) | PhpType::Callable
            if source_ty == PhpType::Mixed && value.ir_type == IrType::Heap(IrHeapKind::Mixed) =>
        {
            let unboxed = ctx.emit_owned_value(
                Op::MixedUnbox,
                vec![value.value],
                None,
                target_ty.clone(),
                Op::mixed_unbox_effects(&target_ty),
                Some(span),
            );
            release_coerced_source_if_owned(ctx, value, Some(span));
            unboxed
        }
        _ => coerce_container_to_mixed_payload(ctx, value, &source_ty, &target_ty, span),
    }
}

/// Widens a typed container value to boxed-Mixed element storage before it is stored.
///
/// Branch merges and stable loop-local contracts can require `Mixed` element storage, so each
/// concrete container must box its slots via `ArrayToMixed` / `HashToMixed`: storing the raw
/// pointer would let Mixed-element reads misinterpret typed slot bytes. Borrowed sources are
/// retained first so the conversion's copy-on-write split rewrites a private copy; owning
/// temporaries transfer their reference into the converted result.
pub(in crate::ir_lower) fn coerce_container_to_mixed_payload(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    source_ty: &PhpType,
    target_ty: &PhpType,
    span: crate::span::Span,
) -> LoweredValue {
    let target_has_mixed_payload = match target_ty {
        PhpType::Array(elem) => elem.codegen_repr() == PhpType::Mixed,
        PhpType::AssocArray { value, .. } => value.codegen_repr() == PhpType::Mixed,
        _ => false,
    };
    if !target_has_mixed_payload {
        // A TYPED container target with a whole-boxed source is the `?array<int>` shape, and it
        // only exists since `array<T>` did: before that a container merge target was always
        // `array<mixed>`, so this early return could only ever see a source that needed nothing.
        // It stored the CELL into the raw slot untouched, and the first element read then saw the
        // cell header — `function g(?array<int> $a) { $r = $a ?? [4, 5]; echo $r[0]; }` printed
        // `0` and dumped four zeros where php prints `1` and `[1, 2, 3]`. A ternary was correct,
        // and a bare `?array` was correct, which is what kept it hidden.
        //
        // Unboxing is exact here: the payload's slots carry the declared element type, because
        // that declaration is what typed the caller's array in the first place. If that is ever
        // false, the boundary guard in `lower_runtime_call` reads the payload's own value_type
        // tag and fatals rather than handing back a cell pointer as an element.
        if matches!(source_ty, PhpType::Mixed | PhpType::Union(_))
            && value.ir_type == IrType::Heap(IrHeapKind::Mixed)
            && matches!(target_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
        {
            let cell_is_owning = ctx.value_is_owning_temporary(value);
            let unboxed = ctx.emit_value(
                Op::RuntimeCall,
                vec![value.value],
                None,
                target_ty.clone(),
                effects_lookup::runtime_effects(),
                Some(span),
            );
            // The unbox takes its own reference to the payload, so an owning cell has no reader
            // left — the same ledger the whole-boxed branch below keeps.
            if cell_is_owning {
                crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
            }
            return unboxed;
        }
        return value;
    }
    let op = match (source_ty, target_ty) {
        (PhpType::Array(source_elem), PhpType::Array(_))
            if source_elem.codegen_repr() != PhpType::Mixed =>
        {
            Op::ArrayToMixed
        }
        (PhpType::AssocArray { value: source_value, .. }, PhpType::AssocArray { .. })
            if source_value.codegen_repr() != PhpType::Mixed =>
        {
            Op::HashToMixed
        }
        (PhpType::Mixed | PhpType::Union(_), _)
            if value.ir_type == IrType::Heap(IrHeapKind::Mixed) =>
        {
            // Whole-boxed sources (a `?array` value flowing through `??`)
            // unbox the cell payload and convert it with the same
            // runtime-call coercion declared container returns use. The
            // conversion borrows the cell and owns a fresh container
            // reference, so an owning cell must be consumed here.
            //
            // The indexed conversion consumes one owned payload reference
            // and rewrites sole-owner arrays in place, which is only sound
            // when the cell owns its payload. A borrowed cell (a `?array`
            // parameter or local) shares its payload with a live caller
            // array, so it unboxes through the owned-payload coercion —
            // which retains the payload — and the consuming `ArrayToMixed`
            // copy-on-write-splits into a private converted copy. The
            // associative helper returns a fresh hash without consuming the
            // payload reference, so borrowed hash cells keep the
            // single-call coercion.
            let cell_is_owning = ctx.value_is_owning_temporary(value);
            if !cell_is_owning && matches!(target_ty, PhpType::Array(_)) {
                let unboxed = ctx.emit_value(
                    Op::RuntimeCall,
                    vec![value.value],
                    None,
                    PhpType::Array(Box::new(PhpType::Never)),
                    effects_lookup::runtime_effects(),
                    Some(span),
                );
                return ctx.emit_value(
                    Op::ArrayToMixed,
                    vec![unboxed.value],
                    None,
                    target_ty.clone(),
                    Op::ArrayToMixed.default_effects(),
                    Some(span),
                );
            }
            let converted = ctx.emit_value(
                Op::RuntimeCall,
                vec![value.value],
                None,
                target_ty.clone(),
                effects_lookup::runtime_effects(),
                Some(span),
            );
            if cell_is_owning {
                crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
            }
            return converted;
        }
        _ => return value,
    };
    // Local loads report as *provisional* owners (their compensating releases
    // are pruned at builder finalization when the slot stays concrete), so
    // they must be treated as borrowed here: without a real retain the
    // conversion's copy-on-write split would never trigger and the local's
    // own array would be boxed in place while its slot type stays concrete.
    let source_is_consumable = ctx.value_is_owning_temporary(value)
        && !ctx.value_is_owned_unboxed_local_load(value.value);
    let source = if source_is_consumable {
        value
    } else {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span))
    };
    ctx.emit_value(
        op,
        vec![source.value],
        None,
        target_ty.clone(),
        op.default_effects(),
        Some(span),
    )
}

/// Emits a branch to a target block when the current block can still fall through.
pub(super) fn branch_to(ctx: &mut LoweringContext<'_, '_>, target: BlockId) {
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br { target, args: Vec::new() });
    }
}

/// Computes definitely initialized slots after a two-way expression split.
pub(super) fn merge_initialized_slots_for_expr(
    split_initialized: &HashSet<LocalSlotId>,
    then_initialized: HashSet<LocalSlotId>,
    then_reachable: bool,
    else_initialized: HashSet<LocalSlotId>,
    else_reachable: bool,
) -> HashSet<LocalSlotId> {
    match (then_reachable, else_reachable) {
        (true, true) => then_initialized
            .intersection(&else_initialized)
            .copied()
            .collect(),
        (true, false) => then_initialized,
        (false, true) => else_initialized,
        (false, false) => split_initialized.clone(),
    }
}

/// Emits a boolean literal value for control-expression lowering.
///
/// Also emits the trailing warn-on-missing flag that boxed-`Mixed` subscript reads
/// pass to `__rt_mixed_array_get`, so every producer of such a read builds the
/// operand the same way.
pub(crate) fn emit_bool_literal(
    ctx: &mut LoweringContext<'_, '_>,
    value: bool,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    let value = ctx
        .builder
        .emit_with_effects(
            Op::ConstBool,
            Vec::new(),
            Some(Immediate::Bool(value)),
            IrType::I64,
            PhpType::Bool,
            Ownership::NonHeap,
            Op::ConstBool.default_effects(),
            span,
        )
        .expect("const_bool produces a value");
    LoweredValue { value, ir_type: IrType::I64 }
}

/// Returns a printable static receiver name.
pub(super) fn receiver_name(receiver: &StaticReceiver) -> String {
    // A receiver written `Box<int>::of()` names `Box` until instantiation renames
    // it, and this pass can run on a generic function's template body — which is
    // walked and then stripped, never instantiated.
    let receiver = &receiver.written_class_receiver();
    match receiver {
        // A generic receiver is instantiated into an ordinary named one before type checking;
        // a template has no class to reach through.
        StaticReceiver::Generic(_) => unreachable!(
            "StaticReceiver::Generic must be instantiated by generics::classes"
        ),
        StaticReceiver::Named(name) => name.as_str().to_string(),
        StaticReceiver::Self_ => "self".to_string(),
        StaticReceiver::Static => "static".to_string(),
        StaticReceiver::Parent => "parent".to_string(),
    }
}

/// Returns a printable callable target name.
pub(super) fn callable_target_name(target: &CallableTarget) -> String {
    match target {
        CallableTarget::Function(name) => name.as_str().to_string(),
        CallableTarget::StaticMethod { receiver, method } => {
            format!("{}::{}", receiver_name(receiver), method)
        }
        CallableTarget::Method { method, .. } => format!("object::{}", method),
    }
}

/// Returns a syntactic fallback PHP type for an expression.
pub(super) fn fallback_expr_type(expr: &Expr) -> PhpType {
    normalize_value_php_type(infer_expr_type_syntactic(expr))
}

/// Normalizes non-materializable expression types to the EIR null sentinel.
pub(super) fn normalize_value_php_type(php_type: PhpType) -> PhpType {
    if matches!(php_type, PhpType::Never) {
        PhpType::Void
    } else {
        php_type
    }
}
