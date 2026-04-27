use super::*;

pub(super) fn block_may_throw(stmts: &[Stmt]) -> bool {
    block_effect(stmts).may_throw
}

pub(super) fn stmt_may_throw(stmt: &Stmt) -> bool {
    stmt_effect(stmt).may_throw
}

pub(super) fn stmt_effect(stmt: &Stmt) -> Effect {
    match &stmt.kind {
        StmtKind::Echo(expr) => expr_effect(expr).with_side_effects(),
        StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::Return(Some(expr)) => expr_effect(expr),
        StmtKind::Throw(expr) => expr_effect(expr).with_side_effects().with_may_throw(),
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::ArrayPush { value, .. }
        | StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => {
            expr_effect(value).with_side_effects()
        }
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            expr_effect(index)
                .combine(expr_effect(value))
                .with_side_effects()
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_effect(object)
                .combine(expr_effect(value))
                .with_side_effects()
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => expr_effect(condition)
            .combine(block_effect(then_body))
            .combine(combine_effects(
                elseif_clauses.iter().map(|(condition, body)| {
                    expr_effect(condition).combine(block_effect(body))
                }),
            ))
            .combine(
                else_body
                    .as_ref()
                    .map(|body| block_effect(body))
                    .unwrap_or(Effect::PURE),
            ),
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => block_effect(then_body).combine(
            else_body
                .as_ref()
                .map(|body| block_effect(body))
                .unwrap_or(Effect::PURE),
        ),
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expr_effect(condition).combine(block_effect(body))
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => init
            .as_ref()
            .map(|stmt| stmt_effect(stmt))
            .unwrap_or(Effect::PURE)
            .combine(
                condition
                    .as_ref()
                    .map(|expr| expr_effect(expr))
                    .unwrap_or(Effect::PURE),
            )
            .combine(
                update
                    .as_ref()
                    .map(|stmt| stmt_effect(stmt))
                    .unwrap_or(Effect::PURE),
            )
            .combine(block_effect(body)),
        StmtKind::Foreach { array, body, .. } => expr_effect(array)
            .combine(block_effect(body))
            .with_side_effects(),
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => expr_effect(subject).combine(combine_effects(cases.iter().map(|(patterns, body)| {
            combine_effects(patterns.iter().map(expr_effect)).combine(block_effect(body))
        })))
        .combine(
            default
                .as_ref()
                .map(|body| block_effect(body))
                .unwrap_or(Effect::PURE),
        ),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => block_effect(try_body)
            .combine(combine_effects(
                catches.iter().map(|catch| block_effect(&catch.body)),
            ))
            .combine(
                finally_body
                    .as_ref()
                    .map(|body| block_effect(body))
                    .unwrap_or(Effect::PURE),
            ),
        StmtKind::NamespaceBlock { body, .. } => block_effect(body),
        StmtKind::FunctionDecl { .. }
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::Global { .. }
        | StmtKind::Return(None)
        | StmtKind::Break
        | StmtKind::Continue
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => Effect::PURE,
        StmtKind::Include { .. } => Effect::PURE.with_side_effects().with_may_throw(),
    }
}

pub(super) fn expr_is_observable(expr: &Expr) -> bool {
    expr_effect(expr).is_observable()
}

pub(super) fn expr_effect(expr: &Expr) -> Effect {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::EnumCase { .. }
        | ExprKind::This => Effect::PURE,
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::Spread(inner) => expr_effect(inner),
        ExprKind::BinaryOp { left, right, .. } => expr_effect(left).combine(expr_effect(right)),
        ExprKind::Throw(inner) => expr_effect(inner).with_side_effects().with_may_throw(),
        ExprKind::NullCoalesce { value, default } => expr_effect(value).combine(expr_effect(default)),
        ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_) => Effect::PURE.with_side_effects(),
        ExprKind::FunctionCall { name, args } => combine_effects(args.iter().map(expr_effect))
            .combine(function_call_effect(name.as_str())),
        ExprKind::ClosureCall { var, args } => combine_effects(args.iter().map(expr_effect))
            .combine(callable_alias_effect(var)),
        ExprKind::ExprCall { callee, args } => expr_effect(callee)
            .combine(combine_effects(args.iter().map(expr_effect)))
            .combine(expr_call_effect(callee)),
        ExprKind::NewObject { args, .. } => combine_effects(args.iter().map(expr_effect))
            .with_side_effects()
            .with_may_throw(),
        ExprKind::MethodCall { object, method, args } => expr_effect(object)
            .combine(combine_effects(args.iter().map(expr_effect)))
            .combine(private_instance_method_call_effect(object, method)),
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => combine_effects(args.iter().map(expr_effect))
            .combine(static_method_call_effect(receiver, method)),
        ExprKind::ArrayLiteral(items) => combine_effects(items.iter().map(expr_effect)),
        ExprKind::ArrayLiteralAssoc(items) => combine_effects(
            items
                .iter()
                .map(|(key, value)| expr_effect(key).combine(expr_effect(value))),
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => expr_effect(subject)
            .combine(combine_effects(arms.iter().map(|(patterns, value)| {
                combine_effects(patterns.iter().map(expr_effect)).combine(expr_effect(value))
            })))
            .combine(
                default
                    .as_ref()
                    .map(|expr| expr_effect(expr))
                    .unwrap_or(Effect::PURE),
            ),
        ExprKind::ArrayAccess { array, index } => expr_effect(array).combine(expr_effect(index)),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => expr_effect(condition)
            .combine(expr_effect(then_expr))
            .combine(expr_effect(else_expr)),
        ExprKind::ShortTernary { value, default } => {
            expr_effect(value).combine(expr_effect(default))
        }
        ExprKind::Closure { .. } => Effect::PURE,
        ExprKind::NamedArg { value, .. } => expr_effect(value),
        ExprKind::PropertyAccess { object, .. } => expr_effect(object),
        ExprKind::StaticPropertyAccess { .. } => Effect::PURE,
        ExprKind::FirstClassCallable(target) => callable_target_effect(target),
        ExprKind::BufferNew { len, .. } => expr_effect(len).with_side_effects(),
        ExprKind::MagicConstant(_) => {
            unreachable!("MagicConstant must be lowered before optimizer passes")
        }
    }
}

pub(super) fn block_effect(stmts: &[Stmt]) -> Effect {
    let mut aliases = current_callable_alias_effects();
    let mut effect = Effect::PURE;
    for stmt in stmts {
        let stmt_effect = with_callable_alias_effects(aliases.clone(), || stmt_effect(stmt));
        effect = effect.combine(stmt_effect);
        apply_stmt_callable_aliases(stmt, &mut aliases);
        if !matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough) {
            break;
        }
    }
    effect
}

pub(super) fn combine_effects(effects: impl IntoIterator<Item = Effect>) -> Effect {
    effects
        .into_iter()
        .fold(Effect::PURE, |acc, effect| acc.combine(effect))
}

pub(super) fn function_call_effect(name: &str) -> Effect {
    ACTIVE_FUNCTION_EFFECTS.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|effects| effects.get(name).copied())
    })
    .unwrap_or_else(|| {
        if is_pure_non_throwing_builtin(name) {
            Effect::PURE
        } else {
            Effect::PURE.with_side_effects().with_may_throw()
        }
    })
}

pub(super) fn closure_body_call_effect(body: &[Stmt]) -> Effect {
    block_effect(body)
}

pub(super) fn expr_call_effect(callee: &Expr) -> Effect {
    match &callee.kind {
        ExprKind::FirstClassCallable(target) => callable_target_call_effect(target),
        ExprKind::Closure { body, .. } => closure_body_call_effect(body),
        _ => Effect::PURE.with_side_effects().with_may_throw(),
    }
}

pub(super) fn callable_alias_effect(name: &str) -> Effect {
    ACTIVE_CALLABLE_ALIAS_EFFECTS.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|effects| effects.get(name).copied())
    })
    .unwrap_or_else(|| Effect::PURE.with_side_effects().with_may_throw())
}

pub(super) fn callable_target_call_effect(target: &CallableTarget) -> Effect {
    match target {
        CallableTarget::Function(name) => function_call_effect(name.as_str()),
        CallableTarget::StaticMethod { receiver, method } => static_method_call_effect(receiver, method),
        CallableTarget::Method { object, method } => {
            expr_effect(object).combine(private_instance_method_call_effect(object, method))
        }
    }
}

pub(super) fn closure_alias_effect(expr: &Expr) -> Option<Effect> {
    match &expr.kind {
        ExprKind::Closure { body, .. } => Some(closure_body_call_effect(body)),
        _ => None,
    }
}

pub(super) fn merge_callable_value_effects(
    effects: impl IntoIterator<Item = Option<Effect>>,
) -> Option<Effect> {
    let mut effects = effects.into_iter();
    let first = effects.next().flatten()?;
    if effects.all(|effect| effect == Some(first)) {
        Some(first)
    } else {
        None
    }
}

pub(super) fn static_method_call_effect(
    receiver: &crate::parser::ast::StaticReceiver,
    method_name: &str,
) -> Effect {
    let Some(class_name) = resolve_static_receiver_class(receiver) else {
        return Effect::PURE.with_side_effects().with_may_throw();
    };

    ACTIVE_STATIC_METHOD_EFFECTS.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|effects| effects.get(&method_effect_key(&class_name, method_name)).copied())
    })
    .unwrap_or_else(|| Effect::PURE.with_side_effects().with_may_throw())
}

pub(super) fn private_instance_method_call_effect(object: &Expr, method_name: &str) -> Effect {
    if !matches!(object.kind, ExprKind::This) {
        return Effect::PURE.with_side_effects().with_may_throw();
    }

    let Some(class_name) = ACTIVE_CLASS_EFFECT_CONTEXT
        .with(|slot| slot.borrow().as_ref().map(|context| context.class_name.clone()))
    else {
        return Effect::PURE.with_side_effects().with_may_throw();
    };

    ACTIVE_PRIVATE_INSTANCE_METHOD_EFFECTS.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|effects| effects.get(&method_effect_key(&class_name, method_name)).copied())
    })
    .unwrap_or_else(|| Effect::PURE.with_side_effects().with_may_throw())
}

pub(super) fn resolve_static_receiver_class(receiver: &crate::parser::ast::StaticReceiver) -> Option<String> {
    match receiver {
        crate::parser::ast::StaticReceiver::Named(class_name) => Some(class_name.as_str().to_string()),
        crate::parser::ast::StaticReceiver::Self_ => ACTIVE_CLASS_EFFECT_CONTEXT
            .with(|slot| slot.borrow().as_ref().map(|context| context.class_name.clone())),
        crate::parser::ast::StaticReceiver::Parent => ACTIVE_CLASS_EFFECT_CONTEXT.with(|slot| {
            slot.borrow()
                .as_ref()
                .and_then(|context| context.parent_name.clone())
        }),
        crate::parser::ast::StaticReceiver::Static => None,
    }
}

pub(super) fn is_pure_non_throwing_builtin(name: &str) -> bool {
    matches!(
        name,
        "strlen"
            | "count"
            | "intval"
            | "floatval"
            | "boolval"
            | "gettype"
            | "is_array"
            | "is_bool"
            | "is_float"
            | "is_int"
            | "is_null"
            | "is_numeric"
            | "is_string"
            | "abs"
            | "min"
            | "max"
            | "floor"
            | "ceil"
            | "round"
            | "sqrt"
            | "pow"
            | "fmod"
            | "fdiv"
            | "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "atan2"
            | "deg2rad"
            | "rad2deg"
            | "sinh"
            | "cosh"
            | "tanh"
            | "log"
            | "log2"
            | "log10"
            | "exp"
            | "hypot"
            | "pi"
            | "number_format"
            | "substr"
            | "strpos"
            | "strrpos"
            | "strstr"
            | "str_replace"
            | "str_ireplace"
            | "substr_replace"
            | "strtolower"
            | "strtoupper"
            | "ucfirst"
            | "lcfirst"
            | "ucwords"
            | "trim"
            | "ltrim"
            | "rtrim"
            | "str_repeat"
            | "strrev"
            | "str_pad"
            | "explode"
            | "implode"
            | "str_split"
            | "strcmp"
            | "strcasecmp"
            | "str_contains"
            | "str_starts_with"
            | "str_ends_with"
            | "ord"
            | "chr"
            | "nl2br"
            | "wordwrap"
            | "addslashes"
            | "stripslashes"
            | "htmlspecialchars"
            | "htmlentities"
            | "html_entity_decode"
            | "urlencode"
            | "urldecode"
            | "rawurlencode"
            | "rawurldecode"
            | "md5"
            | "sha1"
            | "hash"
            | "base64_encode"
            | "base64_decode"
            | "bin2hex"
            | "hex2bin"
            | "ctype_alpha"
            | "ctype_digit"
            | "ctype_alnum"
            | "ctype_space"
            | "array_key_exists"
            | "array_search"
            | "array_keys"
            | "array_values"
            | "array_merge"
            | "array_slice"
            | "array_combine"
            | "array_flip"
            | "array_reverse"
            | "array_unique"
            | "array_column"
            | "array_sum"
            | "array_product"
            | "array_chunk"
            | "array_pad"
            | "array_fill"
            | "array_fill_keys"
            | "array_diff"
            | "array_intersect"
            | "array_diff_key"
            | "array_intersect_key"
            | "range"
            | "json_encode"
            | "json_decode"
            | "json_last_error"
    )
}


pub(super) fn callable_alias_from_expr(expr: &Expr) -> Option<Effect> {
    match &expr.kind {
        ExprKind::FirstClassCallable(target) => Some(callable_target_call_effect(target)),
        ExprKind::Closure { .. } => closure_alias_effect(expr),
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => merge_callable_value_effects([
            callable_alias_from_expr(then_expr),
            callable_alias_from_expr(else_expr),
        ]),
        ExprKind::ShortTernary { value, default } => merge_callable_value_effects([
            callable_alias_from_expr(value),
            callable_alias_from_expr(default),
        ]),
        ExprKind::NullCoalesce { value, default } => merge_callable_value_effects([
            callable_alias_from_expr(value),
            callable_alias_from_expr(default),
        ]),
        ExprKind::Match { arms, default, .. } => merge_callable_value_effects(
            arms.iter()
                .map(|(_, value)| callable_alias_from_expr(value))
                .chain(default.iter().map(|value| callable_alias_from_expr(value))),
        ),
        ExprKind::NamedArg { value, .. } => callable_alias_from_expr(value),
        ExprKind::Variable(name) => ACTIVE_CALLABLE_ALIAS_EFFECTS.with(|slot| {
            slot.borrow()
                .as_ref()
                .and_then(|effects| effects.get(name).copied())
        }),
        _ => None,
    }
}

pub(super) fn update_callable_alias(aliases: &mut HashMap<String, Effect>, name: &str, value: &Expr) {
    if let Some(effect) = callable_alias_from_expr(value) {
        aliases.insert(name.to_string(), effect);
    } else {
        aliases.remove(name);
    }
}

pub(super) fn simulate_catch_callable_aliases(
    catch: &crate::parser::ast::CatchClause,
    mut aliases: HashMap<String, Effect>,
) -> HashMap<String, Effect> {
    if let Some(name) = &catch.variable {
        aliases.remove(name);
    }
    simulate_block_callable_aliases(&catch.body, aliases)
}

pub(super) fn merge_try_callable_alias_paths(
    try_body: &[Stmt],
    catches: &[crate::parser::ast::CatchClause],
    finally_body: Option<&[Stmt]>,
    incoming_aliases: &HashMap<String, Effect>,
) -> HashMap<String, Effect> {
    let mut fallthrough_paths = Vec::new();

    if matches!(block_terminal_effect(try_body), TerminalEffect::FallsThrough) {
        fallthrough_paths.push(simulate_block_callable_aliases(try_body, incoming_aliases.clone()));
    }

    for catch in catches {
        if matches!(block_terminal_effect(&catch.body), TerminalEffect::FallsThrough) {
            fallthrough_paths.push(simulate_catch_callable_aliases(catch, incoming_aliases.clone()));
        }
    }

    if let Some(finally_body) = finally_body {
        fallthrough_paths = fallthrough_paths
            .into_iter()
            .map(|aliases| simulate_block_callable_aliases(finally_body, aliases))
            .collect();
    }

    merge_callable_alias_paths(fallthrough_paths)
}

pub(super) enum SwitchAliasPathOutcome {
    FallsThrough(HashMap<String, Effect>),
    Breaks(HashMap<String, Effect>),
    ExitsCurrentBlock,
}

pub(super) fn simulate_switch_body_callable_aliases(
    body: &[Stmt],
    mut aliases: HashMap<String, Effect>,
) -> SwitchAliasPathOutcome {
    for stmt in body {
        apply_stmt_callable_aliases(stmt, &mut aliases);
        match stmt_terminal_effect(stmt) {
            TerminalEffect::FallsThrough => {}
            TerminalEffect::Breaks => return SwitchAliasPathOutcome::Breaks(aliases),
            TerminalEffect::ExitsCurrentBlock | TerminalEffect::TerminatesMixed => {
                return SwitchAliasPathOutcome::ExitsCurrentBlock;
            }
        }
    }

    SwitchAliasPathOutcome::FallsThrough(aliases)
}

pub(super) fn simulate_switch_entry_callable_aliases(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
    entry_case: Option<usize>,
    incoming_aliases: &HashMap<String, Effect>,
) -> Option<HashMap<String, Effect>> {
    let mut aliases = incoming_aliases.clone();

    if let Some(start_index) = entry_case {
        for (_, body) in cases.iter().skip(start_index) {
            match simulate_switch_body_callable_aliases(body, aliases) {
                SwitchAliasPathOutcome::FallsThrough(updated) => aliases = updated,
                SwitchAliasPathOutcome::Breaks(updated) => return Some(updated),
                SwitchAliasPathOutcome::ExitsCurrentBlock => return None,
            }
        }
    }

    match default {
        Some(default_body) => match simulate_switch_body_callable_aliases(default_body, aliases) {
            SwitchAliasPathOutcome::FallsThrough(updated)
            | SwitchAliasPathOutcome::Breaks(updated) => Some(updated),
            SwitchAliasPathOutcome::ExitsCurrentBlock => None,
        },
        None => Some(aliases),
    }
}

pub(super) fn merge_switch_callable_alias_paths(
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
    incoming_aliases: &HashMap<String, Effect>,
) -> HashMap<String, Effect> {
    let mut fallthrough_paths = Vec::new();

    for case_index in 0..cases.len() {
        if let Some(aliases) =
            simulate_switch_entry_callable_aliases(cases, default, Some(case_index), incoming_aliases)
        {
            fallthrough_paths.push(aliases);
        }
    }

    if let Some(aliases) = simulate_switch_entry_callable_aliases(cases, default, None, incoming_aliases)
    {
        fallthrough_paths.push(aliases);
    }

    merge_callable_alias_paths(fallthrough_paths)
}

pub(super) fn apply_stmt_callable_aliases(stmt: &Stmt, aliases: &mut HashMap<String, Effect>) {
    match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            let effect = with_callable_alias_effects(aliases.clone(), || callable_alias_from_expr(value));
            if let Some(effect) = effect {
                aliases.insert(name.clone(), effect);
            } else {
                aliases.remove(name);
            }
        }
        StmtKind::StaticVar { name, init } => update_callable_alias(aliases, name, init),
        StmtKind::Global { vars } => {
            for var in vars {
                aliases.remove(var);
            }
        }
        StmtKind::ArrayAssign { array, .. } | StmtKind::ArrayPush { array, .. } => {
            aliases.remove(array);
        }
        StmtKind::ListUnpack { vars, .. } => {
            for var in vars {
                aliases.remove(var);
            }
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            let mut fallthrough_paths = Vec::new();
            if matches!(block_terminal_effect(then_body), TerminalEffect::FallsThrough) {
                fallthrough_paths.push(simulate_block_callable_aliases(then_body, aliases.clone()));
            }
            for (_, body) in elseif_clauses {
                if matches!(block_terminal_effect(body), TerminalEffect::FallsThrough) {
                    fallthrough_paths.push(simulate_block_callable_aliases(body, aliases.clone()));
                }
            }
            if let Some(body) = else_body {
                if matches!(block_terminal_effect(body), TerminalEffect::FallsThrough) {
                    fallthrough_paths.push(simulate_block_callable_aliases(body, aliases.clone()));
                }
            } else {
                fallthrough_paths.push(aliases.clone());
            }
            *aliases = merge_callable_alias_paths(fallthrough_paths);
        }
        StmtKind::IfDef {
            then_body, else_body, ..
        } => {
            let mut fallthrough_paths = Vec::new();
            if matches!(block_terminal_effect(then_body), TerminalEffect::FallsThrough) {
                fallthrough_paths.push(simulate_block_callable_aliases(then_body, aliases.clone()));
            }
            match else_body {
                Some(body) if matches!(block_terminal_effect(body), TerminalEffect::FallsThrough) => {
                    fallthrough_paths.push(simulate_block_callable_aliases(body, aliases.clone()));
                }
                None => fallthrough_paths.push(aliases.clone()),
                _ => {}
            }
            *aliases = merge_callable_alias_paths(fallthrough_paths);
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            *aliases = merge_try_callable_alias_paths(
                try_body,
                catches,
                finally_body.as_deref(),
                aliases,
            );
        }
        StmtKind::Switch { cases, default, .. } => {
            *aliases = merge_switch_callable_alias_paths(cases, default.as_deref(), aliases);
        }
        StmtKind::While { .. }
        | StmtKind::DoWhile { .. }
        | StmtKind::For { .. }
        | StmtKind::Foreach { .. }
        | StmtKind::Include { .. } => aliases.clear(),
        _ => {}
    }
}

pub(super) fn simulate_block_callable_aliases(
    body: &[Stmt],
    mut aliases: HashMap<String, Effect>,
) -> HashMap<String, Effect> {
    for stmt in body {
        apply_stmt_callable_aliases(stmt, &mut aliases);
        if !matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough) {
            break;
        }
    }
    aliases
}

pub(super) fn merge_callable_alias_paths(
    mut paths: Vec<HashMap<String, Effect>>,
) -> HashMap<String, Effect> {
    let Some(first) = paths.pop() else {
        return HashMap::new();
    };
    first
        .into_iter()
        .filter(|(name, effect)| {
            paths.iter()
                .all(|path| path.get(name).copied() == Some(*effect))
        })
        .collect()
}
