use super::*;

pub(crate) fn safe_loop_env(
    env: &ConstantEnv,
    conditions: &[Expr],
    body: &[Stmt],
    update: Option<&Stmt>,
) -> ConstantEnv {
    let mut written = HashSet::new();

    for condition in conditions {
        let Some(condition_writes) = expr_local_writes(condition) else {
            return HashMap::new();
        };
        written.extend(condition_writes);
    }

    let Some(body_writes) = block_local_writes(body) else {
        return HashMap::new();
    };
    written.extend(body_writes);

    if let Some(update) = update {
        let Some(update_writes) = stmt_local_writes(update) else {
            return HashMap::new();
        };
        written.extend(update_writes);
    }

    env.iter()
        .filter(|(name, _)| !written.contains(*name))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

pub(crate) fn safe_foreach_env(
    env: &ConstantEnv,
    array: &Expr,
    key_var: Option<&str>,
    value_var: &str,
    body: &[Stmt],
) -> ConstantEnv {
    let Some(mut written) = expr_local_writes(array) else {
        return HashMap::new();
    };

    written.insert(value_var.to_string());
    if let Some(key_var) = key_var {
        written.insert(key_var.to_string());
    }

    let Some(body_writes) = block_local_writes(body) else {
        return HashMap::new();
    };
    written.extend(body_writes);

    env.iter()
        .filter(|(name, _)| !written.contains(*name))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

pub(crate) fn block_local_writes(body: &[Stmt]) -> Option<HashSet<String>> {
    let mut writes = HashSet::new();
    for stmt in body {
        writes.extend(stmt_local_writes(stmt)?);
    }
    Some(writes)
}

pub(crate) fn stmt_local_writes(stmt: &Stmt) -> Option<HashSet<String>> {
    match &stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::Return(Some(expr)) => expr_local_writes(expr),
        StmtKind::Throw(expr) => expr_local_writes(expr),
        StmtKind::Return(None)
        | StmtKind::Break
        | StmtKind::Continue
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::FunctionDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => Some(HashSet::new()),
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            let mut writes = expr_local_writes(value)?;
            writes.insert(name.clone());
            Some(writes)
        }
        StmtKind::ListUnpack { vars, value } => {
            let mut writes = expr_local_writes(value)?;
            writes.extend(vars.iter().cloned());
            Some(writes)
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
        } => {
            let mut writes = expr_local_writes(array)?;
            writes.insert(value_var.clone());
            if let Some(key_var) = key_var {
                writes.insert(key_var.clone());
            }
            writes.extend(block_local_writes(body)?);
            Some(writes)
        }
        StmtKind::While { condition, body } => {
            let mut writes = expr_local_writes(condition)?;
            writes.extend(block_local_writes(body)?);
            Some(writes)
        }
        StmtKind::DoWhile { body, condition } => {
            let mut writes = block_local_writes(body)?;
            writes.extend(expr_local_writes(condition)?);
            Some(writes)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            let mut writes = HashSet::new();
            if let Some(init) = init {
                writes.extend(stmt_local_writes(init)?);
            }
            if let Some(condition) = condition {
                writes.extend(expr_local_writes(condition)?);
            }
            if let Some(update) = update {
                writes.extend(stmt_local_writes(update)?);
            }
            writes.extend(block_local_writes(body)?);
            Some(writes)
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let mut writes = expr_local_writes(condition)?;
            writes.extend(block_local_writes(then_body)?);
            for (elseif_condition, elseif_body) in elseif_clauses {
                writes.extend(expr_local_writes(elseif_condition)?);
                writes.extend(block_local_writes(elseif_body)?);
            }
            if let Some(else_body) = else_body {
                writes.extend(block_local_writes(else_body)?);
            }
            Some(writes)
        }
        StmtKind::IfDef {
            then_body, else_body, ..
        } => {
            let mut writes = block_local_writes(then_body)?;
            if let Some(else_body) = else_body {
                writes.extend(block_local_writes(else_body)?);
            }
            Some(writes)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            let mut writes = expr_local_writes(subject)?;
            for (patterns, body) in cases {
                for pattern in patterns {
                    writes.extend(expr_local_writes(pattern)?);
                }
                writes.extend(block_local_writes(body)?);
            }
            if let Some(default) = default {
                writes.extend(block_local_writes(default)?);
            }
            Some(writes)
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let mut writes = block_local_writes(try_body)?;
            for catch in catches {
                if let Some(variable) = &catch.variable {
                    writes.insert(variable.clone());
                }
                writes.extend(block_local_writes(&catch.body)?);
            }
            if let Some(finally_body) = finally_body {
                writes.extend(block_local_writes(finally_body)?);
            }
            Some(writes)
        }
        StmtKind::NamespaceBlock { body, .. } => block_local_writes(body),
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => {
            let mut writes = expr_local_writes(index)?;
            writes.extend(expr_local_writes(value)?);
            writes.insert(array.clone());
            Some(writes)
        }
        StmtKind::ArrayPush { array, value } => {
            let mut writes = expr_local_writes(value)?;
            writes.insert(array.clone());
            Some(writes)
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => merge_write_sets([
            expr_local_writes(object)?,
            expr_local_writes(value)?,
        ]),
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => expr_local_writes(value),
        StmtKind::StaticPropertyArrayAssign { index, value, .. } => merge_write_sets([
            expr_local_writes(index)?,
            expr_local_writes(value)?,
        ]),
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => merge_write_sets([
            expr_local_writes(object)?,
            expr_local_writes(index)?,
            expr_local_writes(value)?,
        ]),
        StmtKind::StaticVar { .. }
        | StmtKind::Global { .. }
        | StmtKind::Include { .. } => None,
    }
}

pub(crate) fn expr_local_writes(expr: &Expr) -> Option<HashSet<String>> {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::EnumCase { .. }
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::This
        | ExprKind::FirstClassCallable(_)
        | ExprKind::Closure { .. } => Some(HashSet::new()),
        ExprKind::MagicConstant(_) => {
            unreachable!("MagicConstant must be lowered before optimizer passes")
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Spread(inner)
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::Cast { expr: inner, .. } => expr_local_writes(inner),
        ExprKind::BinaryOp { left, right, .. } => merge_write_sets([
            expr_local_writes(left)?,
            expr_local_writes(right)?,
        ]),
        ExprKind::NullCoalesce { value, default } => merge_write_sets([
            expr_local_writes(value)?,
            expr_local_writes(default)?,
        ]),
        ExprKind::ArrayLiteral(items) => items.iter().try_fold(HashSet::new(), |mut acc, item| {
            acc.extend(expr_local_writes(item)?);
            Some(acc)
        }),
        ExprKind::ArrayLiteralAssoc(items) => {
            items.iter().try_fold(HashSet::new(), |mut acc, (key, value)| {
                acc.extend(expr_local_writes(key)?);
                acc.extend(expr_local_writes(value)?);
                Some(acc)
            })
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            let mut writes = expr_local_writes(subject)?;
            for (patterns, value) in arms {
                for pattern in patterns {
                    writes.extend(expr_local_writes(pattern)?);
                }
                writes.extend(expr_local_writes(value)?);
            }
            if let Some(default) = default {
                writes.extend(expr_local_writes(default)?);
            }
            Some(writes)
        }
        ExprKind::ArrayAccess { array, index } => merge_write_sets([
            expr_local_writes(array)?,
            expr_local_writes(index)?,
        ]),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => merge_write_sets([
            expr_local_writes(condition)?,
            expr_local_writes(then_expr)?,
            expr_local_writes(else_expr)?,
        ]),
        ExprKind::ShortTernary { value, default } => merge_write_sets([
            expr_local_writes(value)?,
            expr_local_writes(default)?,
        ]),
        ExprKind::NamedArg { value, .. } => expr_local_writes(value),
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => Some(HashSet::from([name.clone()])),
        ExprKind::FunctionCall { name, args } if name == "unset" && args.len() == 1 => {
            unset_target_name(expr).map(|name| HashSet::from([name]))
        }
        ExprKind::FunctionCall { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::ExprCall { .. }
        | ExprKind::NewObject { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::BufferNew { .. } => None,
        ExprKind::PropertyAccess { object, .. } => expr_local_writes(object),
    }
}

pub(crate) fn merge_write_sets<const N: usize>(sets: [HashSet<String>; N]) -> Option<HashSet<String>> {
    let mut merged = HashSet::new();
    for set in sets {
        merged.extend(set);
    }
    Some(merged)
}

pub(crate) fn unset_target_name(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::FunctionCall { name, args } if name == "unset" && args.len() == 1 => {
            match &args[0].kind {
                ExprKind::Variable(name) => Some(name.clone()),
                _ => None,
            }
        }
        _ => None,
    }
}
