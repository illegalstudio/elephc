//! Purpose:
//! Lowers class-introspection calls whose results are assembled from resolved AOT metadata.
//!
//! Called from:
//! - `super::function_calls::lower_function_call()` before ordinary registry lowering.
//!
//! Key details:
//! - Direct calls accept runtime class-name strings, and `get_class_methods()` also resolves
//!   an object's concrete runtime class before dispatching through the known metadata inventory.
//! - Property defaults are lowered as ordinary EIR expressions and boxed into fresh Mixed cells.

use super::*;

/// Lowers direct class-variable and class-method introspection through AOT metadata dispatch.
pub(super) fn lower_class_introspection(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let kind = match php_symbol_key(name.trim_start_matches('\\')).as_str() {
        "get_class_vars" => ClassIntrospectionKind::Variables,
        "get_class_methods" => ClassIntrospectionKind::Methods,
        _ => return None,
    };
    let argument = class_introspection_argument(args, kind)?;
    if let Some(class_name) = literal_class_argument(argument)
        .and_then(|requested| resolved_class_name(ctx, &requested))
    {
        return Some(materialize_class_introspection(ctx, kind, &class_name, expr));
    }

    let argument = lower_expr(ctx, argument);
    let argument_type = ctx.builder.value_php_type(argument.value).codegen_repr();
    let name = if kind == ClassIntrospectionKind::Methods
        && matches!(argument_type, PhpType::Object(_))
    {
        let target = crate::ir::RuntimeFnId::GetClass;
        let class_name = ctx.emit_value(
            Op::RuntimeCall,
            vec![argument.value],
            Some(Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::Function(target),
            )),
            PhpType::Str,
            target.effects(),
            Some(expr.span),
        );
        release_owned_call_arg_temporaries(
            ctx,
            &[argument.value],
            Some(class_name.value),
            &ReturnArgAlias::Unknown,
            expr.span,
        );
        class_name
    } else {
        argument
    };
    Some(lower_dynamic_class_introspection(ctx, kind, name, expr))
}

/// Identifies the metadata projection produced by one supported introspection builtin.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ClassIntrospectionKind {
    Variables,
    Methods,
}

impl ClassIntrospectionKind {
    /// Returns the PHP parameter name accepted by this builtin.
    fn parameter_name(self) -> &'static str {
        match self {
            Self::Variables => "class",
            Self::Methods => "object_or_class",
        }
    }

    /// Returns the concrete EIR result type shared by every dispatch branch.
    fn result_type(self) -> PhpType {
        match self {
            Self::Variables => PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Mixed),
            },
            Self::Methods => PhpType::Array(Box::new(PhpType::Str)),
        }
    }
}

/// Extracts the single positional or correctly named argument from a checked call.
fn class_introspection_argument<'a>(
    args: &'a [Expr],
    kind: ClassIntrospectionKind,
) -> Option<&'a Expr> {
    let [argument] = args else {
        return None;
    };
    match &argument.kind {
        ExprKind::NamedArg { name, value }
            if php_symbol_key(name) == kind.parameter_name() =>
        {
            Some(value)
        }
        ExprKind::NamedArg { .. } | ExprKind::Spread(_) => None,
        _ => Some(argument),
    }
}

/// Dispatches a runtime class name across all class-like declarations known to AOT lowering.
fn lower_dynamic_class_introspection(
    ctx: &mut LoweringContext<'_, '_>,
    kind: ClassIntrospectionKind,
    name: LoweredValue,
    expr: &Expr,
) -> LoweredValue {
    let name_temp = ctx.declare_owned_hidden_temp(PhpType::Str);
    store_value_into_temp(ctx, &name_temp, PhpType::Str, name, expr.span);
    let name_var = Expr::new(ExprKind::Variable(name_temp.clone()), expr.span);
    let result_type = kind.result_type();
    let result_temp = ctx.declare_owned_hidden_temp(result_type.clone());
    let merge = ctx
        .builder
        .create_named_block("class.introspection.merge", Vec::new());

    for class_name in class_introspection_candidates(ctx) {
        let match_block = ctx
            .builder
            .create_named_block("class.introspection.match", Vec::new());
        let next_block = ctx
            .builder
            .create_named_block("class.introspection.next", Vec::new());
        let condition = class_name_match_expr(&name_var, &class_name, expr.span);
        let condition = lower_expr(ctx, &condition);
        let condition = coerce_to_int_at_span(ctx, condition, Some(expr.span));
        ctx.builder.terminate(Terminator::CondBr {
            cond: condition.value,
            then_target: match_block,
            then_args: Vec::new(),
            else_target: next_block,
            else_args: Vec::new(),
        });

        ctx.builder.position_at_end(match_block);
        let value = materialize_class_introspection(ctx, kind, &class_name, expr);
        store_value_into_temp(ctx, &result_temp, result_type.clone(), value, expr.span);
        branch_to(ctx, merge);
        ctx.builder.position_at_end(next_block);
    }

    lower_invalid_class_introspection_throw(ctx, kind, &name_var, expr);
    ctx.builder.position_at_end(merge);
    ctx.clear_owned_hidden_temp(&name_temp, Some(expr.span));
    take_owned_temp(ctx, &result_temp, expr.span)
}

/// Returns class-like names in deterministic order for runtime-name dispatch.
fn class_introspection_candidates(ctx: &LoweringContext<'_, '_>) -> Vec<String> {
    let mut candidates = ctx
        .classes
        .keys()
        .chain(ctx.interfaces.keys())
        .chain(ctx.enums.keys())
        .chain(ctx.declared_trait_names.iter())
        .cloned()
        .collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| php_symbol_key(candidate.trim_start_matches('\\')));
    candidates.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    candidates
}

/// Builds a case-insensitive match for both bare and leading-backslash class-name strings.
fn class_name_match_expr(name_var: &Expr, class_name: &str, span: Span) -> Expr {
    let bare = class_name.trim_start_matches('\\');
    let direct = dynamic_new_class_name_match_expr(name_var, bare, true, span);
    let qualified = dynamic_new_class_name_match_expr(
        name_var,
        &format!("\\{}", bare),
        true,
        span,
    );
    Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(direct),
            op: BinOp::Or,
            right: Box::new(qualified),
        },
        span,
    )
}

/// Materializes the selected class variable or method inventory.
fn materialize_class_introspection(
    ctx: &mut LoweringContext<'_, '_>,
    kind: ClassIntrospectionKind,
    class_name: &str,
    expr: &Expr,
) -> LoweredValue {
    match kind {
        ClassIntrospectionKind::Variables => materialize_class_vars(ctx, class_name, expr),
        ClassIntrospectionKind::Methods => materialize_class_methods(ctx, class_name, expr),
    }
}

/// Materializes a fresh associative array of class defaults visible in the lexical scope.
fn materialize_class_vars(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    expr: &Expr,
) -> LoweredValue {
    let entries = visible_class_default_entries(ctx, &class_name);
    let hash_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    };
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(entries.len() as u32)),
        hash_ty,
        Op::HashNew.default_effects(),
        Some(expr.span),
    );
    for (property, default) in entries {
        let key = lower_string_literal(ctx, &property, expr);
        let value = match default {
            Some(default) => lower_expr(ctx, &default),
            None => lower_null(ctx, expr),
        };
        let value = box_value_as_mixed(ctx, value, expr.span);
        ctx.emit_void(
            Op::HashSet,
            vec![hash.value, key.value, value.value],
            None,
            Op::HashSet.default_effects(),
            Some(expr.span),
        );
    }
    hash
}

/// Materializes a fresh indexed array of visible method names.
fn materialize_class_methods(
    ctx: &mut LoweringContext<'_, '_>,
    class_name: &str,
    expr: &Expr,
) -> LoweredValue {
    let items = visible_class_method_names(ctx, class_name)
        .into_iter()
        .map(|method| Expr::new(ExprKind::StringLiteral(method), expr.span))
        .collect::<Vec<_>>();
    lower_array_literal_with_expected_type(ctx, &Expr::new(ExprKind::ArrayLiteral(items), expr.span), PhpType::Str)
}

/// Extracts a literal named class from one already normalized argument expression.
fn literal_class_argument(argument: &Expr) -> Option<String> {
    match &argument.kind {
        ExprKind::StringLiteral(class_name) => {
            Some(class_name.trim_start_matches('\\').to_string())
        }
        ExprKind::ClassConstant { receiver } => match receiver {
            StaticReceiver::Named(name) => {
                Some(name.as_str().trim_start_matches('\\').to_string())
            }
            StaticReceiver::Self_ | StaticReceiver::Static | StaticReceiver::Parent => None,
        },
        _ => None,
    }
}

/// Resolves a case-insensitive class-like name to its canonical declaration spelling.
fn resolved_class_name(ctx: &LoweringContext<'_, '_>, requested: &str) -> Option<String> {
    let key = php_symbol_key(requested);
    ctx.classes
        .keys()
        .chain(ctx.interfaces.keys())
        .chain(ctx.enums.keys())
        .chain(ctx.declared_trait_names.iter())
        .find(|candidate| php_symbol_key(candidate.trim_start_matches('\\')) == key)
        .cloned()
}

/// Collects method names visible from the current lexical class.
fn visible_class_method_names(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
) -> Vec<String> {
    let mut names = if let Some(info) = ctx.classes.get(class_name) {
        info.methods
            .keys()
            .chain(info.static_methods.keys())
            .filter(|method| class_method_visible(ctx, class_name, info, method))
            .map(|method| class_method_display_name(ctx, class_name, info, method))
            .collect::<Vec<_>>()
    } else if let Some(info) = ctx.interfaces.get(class_name) {
        info.method_order
            .iter()
            .chain(info.static_method_order.iter())
            .cloned()
            .collect::<Vec<_>>()
    } else if let Some(methods) = ctx
        .declared_trait_methods
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(class_name))
        .map(|(_, methods)| methods)
    {
        methods
            .iter()
            .filter(|(_, method)| method.visibility == Visibility::Public)
            .map(|(_, method)| method.name.clone())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    names.sort_unstable();
    names.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    names
}

/// Restores the source spelling of a flattened method-map key from its declaring class.
fn class_method_display_name(
    ctx: &LoweringContext<'_, '_>,
    lookup_class: &str,
    info: &crate::types::ClassInfo,
    method: &str,
) -> String {
    let declaring_class = info
        .method_declaring_classes
        .get(method)
        .or_else(|| info.static_method_declaring_classes.get(method))
        .map(String::as_str)
        .unwrap_or(lookup_class);
    ctx.classes
        .get(declaring_class)
        .into_iter()
        .flat_map(|declaring| declaring.method_decls.iter())
        .find(|declaration| php_symbol_key(&declaration.name) == php_symbol_key(method))
        .map(|declaration| declaration.name.clone())
        .unwrap_or_else(|| method.to_string())
}

/// Returns whether one class method is visible from the current lexical class.
fn class_method_visible(
    ctx: &LoweringContext<'_, '_>,
    lookup_class: &str,
    info: &crate::types::ClassInfo,
    method: &str,
) -> bool {
    let (visibility, declaring_class) = if info.methods.contains_key(method) {
        (
            info.method_visibilities
                .get(method)
                .unwrap_or(&Visibility::Public),
            info.method_declaring_classes
                .get(method)
                .map(String::as_str)
                .unwrap_or(lookup_class),
        )
    } else {
        (
            info.static_method_visibilities
                .get(method)
                .unwrap_or(&Visibility::Public),
            info.static_method_declaring_classes
                .get(method)
                .map(String::as_str)
                .unwrap_or(lookup_class),
        )
    };
    property_visible(ctx, declaring_class, visibility)
}

/// Throws PHP's catchable TypeError for a runtime string that names no known class-like symbol.
fn lower_invalid_class_introspection_throw(
    ctx: &mut LoweringContext<'_, '_>,
    kind: ClassIntrospectionKind,
    name_var: &Expr,
    expr: &Expr,
) {
    let message = match kind {
        ClassIntrospectionKind::Variables => Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::new(
                    ExprKind::BinaryOp {
                        left: Box::new(Expr::new(
                            ExprKind::StringLiteral(
                                "get_class_vars(): Argument #1 ($class) must be a valid class name, "
                                    .to_string(),
                            ),
                            expr.span,
                        )),
                        op: BinOp::Concat,
                        right: Box::new(name_var.clone()),
                    },
                    expr.span,
                )),
                op: BinOp::Concat,
                right: Box::new(Expr::new(
                    ExprKind::StringLiteral(" given".to_string()),
                    expr.span,
                )),
            },
            expr.span,
        ),
        ClassIntrospectionKind::Methods => Expr::new(
            ExprKind::StringLiteral(
                "get_class_methods(): Argument #1 ($object_or_class) must be an object or a valid class name, string given"
                    .to_string(),
            ),
            expr.span,
        ),
    };
    let exception = Expr::new(
        ExprKind::NewObject {
            class_name: Name::unqualified("TypeError"),
            args: vec![message],
        },
        expr.span,
    );
    let exception = lower_expr(ctx, &exception);
    ctx.clear_owned_hidden_temp(
        match &name_var.kind {
            ExprKind::Variable(name) => name,
            _ => unreachable!("class introspection name must use a hidden temporary"),
        },
        Some(expr.span),
    );
    ctx.builder.terminate(Terminator::Throw {
        value: exception.value,
    });
}

/// Collects visible instance and static property defaults in physical declaration order.
fn visible_class_default_entries(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
) -> Vec<(String, Option<Expr>)> {
    if ctx.interfaces.contains_key(class_name) {
        return Vec::new();
    }
    if ctx.enums.contains_key(class_name) && !ctx.classes.contains_key(class_name) {
        let mut entries = vec![("name".to_string(), None)];
        if ctx
            .enums
            .get(class_name)
            .is_some_and(|info| info.backing_type.is_some())
        {
            entries.push(("value".to_string(), None));
        }
        return entries;
    }
    let Some(info) = ctx.classes.get(class_name) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    for (index, (property, _)) in info.properties.iter().enumerate() {
        if !seen.insert(property.clone()) || !instance_property_visible(ctx, class_name, info, property)
        {
            continue;
        }
        entries.push((property.clone(), info.defaults.get(index).cloned().flatten()));
    }
    for (index, (property, _)) in info.static_properties.iter().enumerate() {
        if !seen.insert(property.clone()) || !static_property_visible(ctx, class_name, info, property)
        {
            continue;
        }
        entries.push((
            property.clone(),
            info.static_defaults.get(index).cloned().flatten(),
        ));
    }
    entries
}

/// Returns whether an instance property is visible from the current lexical class.
fn instance_property_visible(
    ctx: &LoweringContext<'_, '_>,
    lookup_class: &str,
    info: &crate::types::ClassInfo,
    property: &str,
) -> bool {
    let declaring = info
        .property_declaring_classes
        .get(property)
        .map(String::as_str)
        .unwrap_or(lookup_class);
    let visibility = info
        .property_visibilities
        .get(property)
        .unwrap_or(&Visibility::Public);
    property_visible(ctx, declaring, visibility)
}

/// Returns whether a static property is visible from the current lexical class.
fn static_property_visible(
    ctx: &LoweringContext<'_, '_>,
    lookup_class: &str,
    info: &crate::types::ClassInfo,
    property: &str,
) -> bool {
    let declaring = info
        .static_property_declaring_classes
        .get(property)
        .map(String::as_str)
        .unwrap_or(lookup_class);
    let visibility = info
        .static_property_visibilities
        .get(property)
        .unwrap_or(&Visibility::Public);
    property_visible(ctx, declaring, visibility)
}

/// Applies PHP property visibility to one reflected default entry.
fn property_visible(
    ctx: &LoweringContext<'_, '_>,
    declaring_class: &str,
    visibility: &Visibility,
) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Private => ctx.current_class.as_deref() == Some(declaring_class),
        Visibility::Protected => ctx.current_class.as_deref().is_some_and(|current| {
            current == declaring_class || class_is_descendant(ctx, current, declaring_class)
        }),
    }
}

/// Returns whether one resolved class descends from another.
fn class_is_descendant(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    ancestor: &str,
) -> bool {
    let mut current = ctx.classes.get(class_name).and_then(|info| info.parent.as_deref());
    while let Some(parent) = current {
        if parent == ancestor {
            return true;
        }
        current = ctx.classes.get(parent).and_then(|info| info.parent.as_deref());
    }
    false
}
