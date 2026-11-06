//! Purpose:
//! Borrows stable subjects for bounded string reads without whole-string operand copies.
//!
//! Called from:
//! - Builtin argument lowering before the general source-evaluation owner ledger.
//!
//! Key details:
//! - Uses the shared argument planner for parameter order and defaults.
//! - Admits only string reads and exact scalar arguments that cannot run user code.
//! - Property receiver owners stay published until the builtin result is stabilized.

use super::*;
use crate::ir::RuntimeFnId;

/// Lowers a proved read-only string call, leaving all other argument forms to ordinary lowering.
pub(super) fn try_lower(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    signature: Option<&FunctionSig>,
    args: &[Expr],
) -> Option<Vec<ValueId>> {
    use crate::builtins::semantics::BuiltinLowering;
    use crate::ir::RuntimeCallTarget;
    use crate::types::call_args::{PlannedRegularArg, PlannedSourceValue};
    if !ctx.borrowed_string_calls_enabled || ctx.has_eval_barrier() || ctx.eval_executed() { return None; }
    let def = crate::builtins::registry::lookup(name.trim_start_matches('\\'))?;
    match def.spec.semantics.lowering {
        BuiltinLowering::Runtime(RuntimeCallTarget::Function(target)
            | RuntimeCallTarget::ProfiledFunction { target, .. })
            if borrowable_string_read_target(target) => {}
        _ => return None,
    }
    if args.iter().any(is_spread_arg) { return None; }
    let sig = signature?;
    let span = args.first()?.span;
    let plan = crate::types::call_args::plan_call_args(sig, args, span, true, false).ok()?;
    if plan.has_spread_args() || !plan.variadic_args.is_empty() { return None; }
    // The planner keeps a plain positional call as a passthrough. Named calls carry
    // the separate source-to-parameter mapping, including any omitted defaults.
    let positional = plan.first_named_pos.is_none();
    let mut reads = Vec::new();
    if positional {
        if plan.source_args.len() > crate::types::call_args::regular_param_count(sig) { return None; }
        for (index, expr) in plan.source_args.iter().enumerate() { reads.push((index, index, expr)); }
    } else {
        for source in &plan.source_values {
            let PlannedSourceValue::Regular { source_index, param_idx, expr } = source else { return None; };
            reads.push((*source_index, *param_idx, expr));
        }
    }
    for (_, param_idx, expr) in &reads {
        let string = parameter_accepts_string(sig, *param_idx);
        if !safe_read(ctx, expr, string, *param_idx == 0) { return None; }
    }
    for arg in &plan.regular_args {
        if let PlannedRegularArg::Default(expr) = arg {
            if !matches!(
                expr.kind,
                ExprKind::IntLiteral(_)
                    | ExprKind::FloatLiteral(_)
                    | ExprKind::BoolLiteral(_)
                    | ExprKind::StringLiteral(_)
                    | ExprKind::Null
            ) {
                return None;
            }
        }
    }
    let mut sources = vec![None; plan.source_args.len()];
    for (source_index, param_idx, expr) in reads {
        let value = if let ExprKind::PropertyAccess { object, property } = &expr.kind {
            let object = lower_expr(ctx, object);
            let receiver = root_evaluated_call_argument(ctx, object, expr.span);
            let data = ctx.intern_string(property);
            let value = ctx.emit_value(Op::PropGet, vec![receiver.value], Some(Immediate::Data(data)),
                PhpType::Str, Op::PropGet.default_effects(), Some(expr.span));
            ctx.builder.set_value_ownership(value.value, Ownership::Borrowed);
            value.value
        } else {
            lower_arg_with_signature(ctx, sig, param_idx, expr)
        };
        sources[source_index] = Some(value);
    }
    let operands: Vec<_> = if positional {
        sources.into_iter().map(Option::unwrap).collect()
    } else { plan.regular_args.iter().enumerate().map(|(index, arg)| match arg {
        PlannedRegularArg::Source { source_index, .. } => sources[*source_index].unwrap(),
        PlannedRegularArg::Default(expr) => lower_arg_with_signature(ctx, sig, index, expr),
        PlannedRegularArg::SpreadElement { .. } => unreachable!(),
    }).collect() };
    if operands.is_empty() { return None; }
    for operand in &operands {
        if ctx.builder.value_php_type(*operand).codegen_repr() == PhpType::Str
            && ctx.builder.value_defining_op(*operand) != Some(Op::ConstStr)
        {
            ctx.borrowed_string_call_operands.insert(*operand);
        }
    }
    Some(operands)
}

/// Returns whether a read-only builtin may take every string argument by borrow.
///
/// The list contains mbstring functions whose observable work never re-enters PHP code:
/// length/width queries, case conversion, trimming, search, slicing, splitting, encoding
/// conversion, and regex replacement. Callback-taking variants stay out because they can
/// mutate or free the subject while the builtin is still reading it.
fn borrowable_string_read_target(target: RuntimeFnId) -> bool {
    matches!(
        target,
        RuntimeFnId::Substr
            | RuntimeFnId::SubstrCount
            | RuntimeFnId::Strpos
            | RuntimeFnId::Stripos
            | RuntimeFnId::Strrpos
            | RuntimeFnId::Strripos
            | RuntimeFnId::StrContains
            | RuntimeFnId::StrStartsWith
            | RuntimeFnId::StrEndsWith
            | RuntimeFnId::MbStrlen
            | RuntimeFnId::MbStrwidth
            | RuntimeFnId::MbStrtoupper
            | RuntimeFnId::MbStrtolower
            | RuntimeFnId::MbConvertCase
            | RuntimeFnId::MbUcfirst
            | RuntimeFnId::MbLcfirst
            | RuntimeFnId::MbStrimwidth
            | RuntimeFnId::MbSubstr
            | RuntimeFnId::MbStrcut
            | RuntimeFnId::MbScrub
            | RuntimeFnId::MbTrim
            | RuntimeFnId::MbLtrim
            | RuntimeFnId::MbRtrim
            | RuntimeFnId::MbStrPad
            | RuntimeFnId::MbSubstrCount
            | RuntimeFnId::MbOrd
            | RuntimeFnId::MbChr
            | RuntimeFnId::MbStrpos
            | RuntimeFnId::MbStripos
            | RuntimeFnId::MbStrrpos
            | RuntimeFnId::MbStrripos
            | RuntimeFnId::MbStrstr
            | RuntimeFnId::MbStristr
            | RuntimeFnId::MbStrrchr
            | RuntimeFnId::MbStrrichr
            | RuntimeFnId::MbStrSplit
            | RuntimeFnId::MbCheckEncoding
            | RuntimeFnId::MbDetectEncoding
            | RuntimeFnId::MbConvertEncoding
            | RuntimeFnId::MbEncodeNumericentity
            | RuntimeFnId::MbDecodeNumericentity
            | RuntimeFnId::MbEncodeMimeheader
            | RuntimeFnId::MbConvertKana
            | RuntimeFnId::MbOutputHandler
            | RuntimeFnId::MbPreferredMimeName
            | RuntimeFnId::MbEncodingAliases
            | RuntimeFnId::MbGetInfo
            | RuntimeFnId::MbInternalEncoding
            | RuntimeFnId::MbLanguage
            | RuntimeFnId::MbHttpOutput
            | RuntimeFnId::MbRegexEncoding
            | RuntimeFnId::MbRegexSetOptions
            | RuntimeFnId::MbHttpInput
            | RuntimeFnId::MbSplit
            | RuntimeFnId::MbEregReplace
            | RuntimeFnId::MbEregSearchInit
    )
}

/// Returns whether the parameter at `index` accepts a string without implicit object conversion.
///
/// A nullable or mixed-shape union such as `array|string|null` passes a string operand through
/// unchanged, so borrowing stays sound. Only a parameter that cannot accept a string is excluded;
/// [`safe_read`] still refuses any argument that would require a scalar or object coercion.
fn parameter_accepts_string(sig: &FunctionSig, index: usize) -> bool {
    let Some((_, ty)) = sig.params.get(index) else { return false };
    match ty {
        PhpType::Str => true,
        PhpType::Union(members) => members.iter().any(|member| matches!(member, PhpType::Str)),
        _ => false,
    }
}

/// Proves that an argument has its exact scalar representation and cannot invalidate a string.
fn safe_read(ctx: &LoweringContext<'_, '_>, expr: &Expr, string: bool, property_allowed: bool) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_) => true,
        ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) | ExprKind::BoolLiteral(_) => !string,
        ExprKind::Null => true,
        ExprKind::ConstRef(name) => ctx.constant_value(name).is_some_and(|(value, _)| {
            matches!(value, ExprKind::StringLiteral(_)) == string
        }),
        ExprKind::Variable(name) => !ctx.local_uses_global_storage(name)
            && !ctx.extern_globals.contains_key(name)
            && {
                let representation = ctx.local_type(name).codegen_repr();
                if string {
                    representation == PhpType::Str
                } else if matches!(representation, PhpType::Int | PhpType::Bool | PhpType::Float) {
                    true
                } else {
                    matches!(&representation, PhpType::Array(inner) if inner.codegen_repr() == PhpType::Int)
                }
            },
        ExprKind::PropertyAccess { object, property } if string && property_allowed => {
            let name = match &object.kind {
                ExprKind::Variable(name) => name.as_str(),
                ExprKind::This => "this",
                _ => return false,
            };
            if ctx.local_uses_global_storage(name) || ctx.extern_globals.contains_key(name) { return false; }
            let PhpType::Object(class) = ctx.local_type(name).codegen_repr() else { return false; };
            let Some(info) = ctx.classes.get(class.trim_start_matches('\\')) else { return false; };
            if runtime_property_type_override(ctx, &class, property).is_some() { return false; }
            let mut ancestor = Some(info);
            while let Some(info) = ancestor {
                if info.is_internal { return false; }
                ancestor = info.parent.as_ref().and_then(|parent| ctx.classes.get(parent.as_str()));
            }
            info.visible_property(property).is_some_and(|(_, (_, ty))| ty.codegen_repr() == PhpType::Str)
                && !info.methods.contains_key(&php_symbol_key(&property_hook_get_method(property)))
                && matches!(crate::types::resolve_property_name(ctx.classes, &class, property,
                    ctx.current_class.as_deref()), crate::types::PropertyNameResolution::Visible)
        }
        _ => false,
    }
}
