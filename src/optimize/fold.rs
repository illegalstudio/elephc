use super::*;

pub(super) fn fold_params(
    params: Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)>,
) -> Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)> {
    params
        .into_iter()
        .map(|(name, type_expr, default, is_ref)| {
            (name, type_expr, default.map(fold_expr), is_ref)
        })
        .collect()
}

pub(super) fn fold_property(property: ClassProperty) -> ClassProperty {
    ClassProperty {
        name: property.name,
        visibility: property.visibility,
        type_expr: property.type_expr,
        readonly: property.readonly,
        is_final: property.is_final,
        is_static: property.is_static,
        by_ref: property.by_ref,
        default: property.default.map(fold_expr),
        span: property.span,
    }
}

pub(super) fn fold_method(method: ClassMethod) -> ClassMethod {
    ClassMethod {
        name: method.name,
        visibility: method.visibility,
        is_static: method.is_static,
        is_abstract: method.is_abstract,
        is_final: method.is_final,
        has_body: method.has_body,
        params: fold_params(method.params),
        variadic: method.variadic,
        return_type: method.return_type,
        body: fold_block(method.body),
        span: method.span,
    }
}

pub(super) fn fold_enum_case(case: EnumCaseDecl) -> EnumCaseDecl {
    EnumCaseDecl {
        name: case.name,
        value: case.value.map(fold_expr),
        span: case.span,
    }
}

pub(super) fn fold_expr(expr: Expr) -> Expr {
    let span = expr.span;
    let kind = match expr.kind {
        ExprKind::StringLiteral(value) => ExprKind::StringLiteral(value),
        ExprKind::IntLiteral(value) => ExprKind::IntLiteral(value),
        ExprKind::FloatLiteral(value) => ExprKind::FloatLiteral(value),
        ExprKind::Variable(name) => ExprKind::Variable(name),
        ExprKind::BinaryOp { left, op, right } => {
            let left = fold_expr(*left);
            let right = fold_expr(*right);
            try_fold_binary_op(&op, &left, &right).unwrap_or_else(|| ExprKind::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            })
        }
        ExprKind::InstanceOf { value, target } => ExprKind::InstanceOf {
            value: Box::new(fold_expr(*value)),
            target,
        },
        ExprKind::BoolLiteral(value) => ExprKind::BoolLiteral(value),
        ExprKind::Null => ExprKind::Null,
        ExprKind::Negate(inner) => {
            let inner = fold_expr(*inner);
            try_fold_negate(&inner).unwrap_or_else(|| ExprKind::Negate(Box::new(inner)))
        }
        ExprKind::Not(inner) => {
            let inner = fold_expr(*inner);
            try_fold_not(&inner).unwrap_or_else(|| ExprKind::Not(Box::new(inner)))
        }
        ExprKind::BitNot(inner) => {
            let inner = fold_expr(*inner);
            try_fold_bit_not(&inner).unwrap_or_else(|| ExprKind::BitNot(Box::new(inner)))
        }
        ExprKind::Throw(inner) => ExprKind::Throw(Box::new(fold_expr(*inner))),
        ExprKind::NullCoalesce { value, default } => {
            let value = fold_expr(*value);
            let default = fold_expr(*default);
            try_fold_null_coalesce(&value, &default).unwrap_or_else(|| ExprKind::NullCoalesce {
                value: Box::new(value),
                default: Box::new(default),
            })
        }
        ExprKind::PreIncrement(name) => ExprKind::PreIncrement(name),
        ExprKind::PostIncrement(name) => ExprKind::PostIncrement(name),
        ExprKind::PreDecrement(name) => ExprKind::PreDecrement(name),
        ExprKind::PostDecrement(name) => ExprKind::PostDecrement(name),
        ExprKind::FunctionCall { name, args } => ExprKind::FunctionCall {
            name,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::ArrayLiteral(items) => {
            ExprKind::ArrayLiteral(items.into_iter().map(fold_expr).collect())
        }
        ExprKind::ArrayLiteralAssoc(items) => ExprKind::ArrayLiteralAssoc(
            items.into_iter()
                .map(|(key, value)| (fold_expr(key), fold_expr(value)))
                .collect(),
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            let subject = fold_expr(*subject);
            let arms = arms
                .into_iter()
                .map(|(patterns, value)| {
                    (
                        patterns.into_iter().map(fold_expr).collect(),
                        fold_expr(value),
                    )
                })
                .collect();
            let default = default.map(|expr| Box::new(fold_expr(*expr)));
            try_prune_match_expr(subject, arms, default)
        }
        ExprKind::ArrayAccess { array, index } => {
            let array = fold_expr(*array);
            let index = fold_expr(*index);
            try_fold_array_access(&array, &index).unwrap_or_else(|| ExprKind::ArrayAccess {
                array: Box::new(array),
                index: Box::new(index),
            })
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            let condition = fold_expr(*condition);
            let then_expr = fold_expr(*then_expr);
            let else_expr = fold_expr(*else_expr);
            try_fold_ternary(&condition, &then_expr, &else_expr).unwrap_or_else(|| {
                ExprKind::Ternary {
                    condition: Box::new(condition),
                    then_expr: Box::new(then_expr),
                    else_expr: Box::new(else_expr),
                }
            })
        }
        ExprKind::ShortTernary { value, default } => {
            let value = fold_expr(*value);
            let default = fold_expr(*default);
            try_fold_short_ternary(&value, &default).unwrap_or_else(|| ExprKind::ShortTernary {
                value: Box::new(value),
                default: Box::new(default),
            })
        }
        ExprKind::Cast { target, expr } => {
            let expr = fold_expr(*expr);
            try_fold_cast(&target, &expr).unwrap_or_else(|| ExprKind::Cast {
                target,
                expr: Box::new(expr),
            })
        }
        ExprKind::Closure {
            params,
            variadic,
            body,
            is_arrow,
            captures,
        } => ExprKind::Closure {
            params: fold_params(params),
            variadic,
            body: fold_block(body),
            is_arrow,
            captures,
        },
        ExprKind::NamedArg { name, value } => ExprKind::NamedArg {
            name,
            value: Box::new(fold_expr(*value)),
        },
        ExprKind::Spread(inner) => ExprKind::Spread(Box::new(fold_expr(*inner))),
        ExprKind::ClosureCall { var, args } => ExprKind::ClosureCall {
            var,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::ExprCall { callee, args } => ExprKind::ExprCall {
            callee: Box::new(fold_expr(*callee)),
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::ConstRef(name) => ExprKind::ConstRef(name),
        ExprKind::EnumCase {
            enum_name,
            case_name,
        } => ExprKind::EnumCase {
            enum_name,
            case_name,
        },
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(fold_expr(*object)),
            property,
        },
        ExprKind::NullsafePropertyAccess { object, property } => {
            ExprKind::NullsafePropertyAccess {
                object: Box::new(fold_expr(*object)),
                property,
            }
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            ExprKind::StaticPropertyAccess { receiver, property }
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => ExprKind::MethodCall {
            object: Box::new(fold_expr(*object)),
            method,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => ExprKind::NullsafeMethodCall {
            object: Box::new(fold_expr(*object)),
            method,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => ExprKind::StaticMethodCall {
            receiver,
            method,
            args: args.into_iter().map(fold_expr).collect(),
        },
        ExprKind::FirstClassCallable(target) => {
            ExprKind::FirstClassCallable(fold_callable_target(target))
        }
        ExprKind::This => ExprKind::This,
        ExprKind::PtrCast { target_type, expr } => ExprKind::PtrCast {
            target_type,
            expr: Box::new(fold_expr(*expr)),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type,
            len: Box::new(fold_expr(*len)),
        },
        ExprKind::MagicConstant(_) => {
            unreachable!("MagicConstant must be lowered before optimizer passes")
        }
    };
    Expr { kind, span }
}

pub(super) fn fold_callable_target(target: CallableTarget) -> CallableTarget {
    match target {
        CallableTarget::Function(name) => CallableTarget::Function(name),
        CallableTarget::StaticMethod { receiver, method } => {
            CallableTarget::StaticMethod { receiver, method }
        }
        CallableTarget::Method { object, method } => CallableTarget::Method {
            object: Box::new(fold_expr(*object)),
            method,
        },
    }
}

pub(super) fn try_fold_negate(expr: &Expr) -> Option<ExprKind> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => value.checked_neg().map(ExprKind::IntLiteral),
        ExprKind::FloatLiteral(value) => Some(ExprKind::FloatLiteral(-value)),
        _ => None,
    }
}

pub(super) fn try_fold_not(expr: &Expr) -> Option<ExprKind> {
    Some(ExprKind::BoolLiteral(!scalar_value(expr)?.truthy()))
}

pub(super) fn try_fold_bit_not(expr: &Expr) -> Option<ExprKind> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(ExprKind::IntLiteral(!value)),
        _ => None,
    }
}

pub(super) fn try_fold_binary_op(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    match op {
        BinOp::Concat => try_fold_concat(left, right),
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow => {
            try_fold_numeric_binop(op, left, right)
        }
        BinOp::Mod => try_fold_int_mod(left, right),
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight => {
            try_fold_bitwise_binop(op, left, right)
        }
        BinOp::And | BinOp::Or | BinOp::Xor => try_fold_logical_binop(op, left, right),
        BinOp::Eq
        | BinOp::NotEq
        | BinOp::StrictEq
        | BinOp::StrictNotEq
        | BinOp::Lt
        | BinOp::Gt
        | BinOp::LtEq
        | BinOp::GtEq
        | BinOp::Spaceship => try_fold_compare_binop(op, left, right),
        _ => None,
    }
}

pub(super) fn try_fold_concat(left: &Expr, right: &Expr) -> Option<ExprKind> {
    let ExprKind::StringLiteral(left) = &left.kind else {
        return None;
    };
    let ExprKind::StringLiteral(right) = &right.kind else {
        return None;
    };
    Some(ExprKind::StringLiteral(format!("{left}{right}")))
}

pub(super) fn try_fold_numeric_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    if let (Some(left), Some(right)) = (int_literal(left), int_literal(right)) {
        return try_fold_int_numeric_binop(op, left, right);
    }

    let (left, right) = (numeric_literal(left)?, numeric_literal(right)?);
    if matches!(op, BinOp::Div) && right == 0.0 {
        return None;
    }
    let result = match op {
        BinOp::Add => left + right,
        BinOp::Sub => left - right,
        BinOp::Mul => left * right,
        BinOp::Div => left / right,
        BinOp::Pow => left.powf(right),
        _ => return None,
    };
    if result.is_finite() {
        Some(ExprKind::FloatLiteral(result))
    } else {
        None
    }
}

pub(super) fn try_fold_int_numeric_binop(op: &BinOp, left: i64, right: i64) -> Option<ExprKind> {
    match op {
        BinOp::Add => left.checked_add(right).map(ExprKind::IntLiteral),
        BinOp::Sub => left.checked_sub(right).map(ExprKind::IntLiteral),
        BinOp::Mul => left.checked_mul(right).map(ExprKind::IntLiteral),
        BinOp::Div => {
            if right == 0 {
                None
            } else {
                Some(ExprKind::FloatLiteral(left as f64 / right as f64))
            }
        }
        BinOp::Pow => {
            let result = (left as f64).powf(right as f64);
            if result.is_finite() {
                Some(ExprKind::FloatLiteral(result))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(super) fn try_fold_int_mod(left: &Expr, right: &Expr) -> Option<ExprKind> {
    let (left, right) = (int_literal(left)?, int_literal(right)?);
    if right == 0 {
        None
    } else {
        Some(ExprKind::IntLiteral(left % right))
    }
}

pub(super) fn try_fold_bitwise_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    let (left, right) = (int_literal(left)?, int_literal(right)?);
    match op {
        BinOp::BitAnd => Some(ExprKind::IntLiteral(left & right)),
        BinOp::BitOr => Some(ExprKind::IntLiteral(left | right)),
        BinOp::BitXor => Some(ExprKind::IntLiteral(left ^ right)),
        BinOp::ShiftLeft => {
            let shift = u32::try_from(right).ok()?;
            left.checked_shl(shift).map(ExprKind::IntLiteral)
        }
        BinOp::ShiftRight => {
            let shift = u32::try_from(right).ok()?;
            left.checked_shr(shift).map(ExprKind::IntLiteral)
        }
        _ => None,
    }
}

pub(super) fn try_fold_logical_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    let left = scalar_value(left)?;
    let right = scalar_value(right)?;
    let result = match op {
        BinOp::And => left.truthy() && right.truthy(),
        BinOp::Or => left.truthy() || right.truthy(),
        BinOp::Xor => left.truthy() ^ right.truthy(),
        _ => return None,
    };
    Some(ExprKind::BoolLiteral(result))
}

pub(super) fn try_fold_compare_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    match op {
        BinOp::Eq => Some(ExprKind::BoolLiteral(loose_eq(left, right)?)),
        BinOp::NotEq => Some(ExprKind::BoolLiteral(!loose_eq(left, right)?)),
        BinOp::StrictEq => Some(ExprKind::BoolLiteral(strict_eq(left, right)?)),
        BinOp::StrictNotEq => Some(ExprKind::BoolLiteral(!strict_eq(left, right)?)),
        BinOp::Lt => Some(ExprKind::BoolLiteral(compare_numeric(left, right, |l, r| l < r)?)),
        BinOp::Gt => Some(ExprKind::BoolLiteral(compare_numeric(left, right, |l, r| l > r)?)),
        BinOp::LtEq => Some(ExprKind::BoolLiteral(compare_numeric(left, right, |l, r| l <= r)?)),
        BinOp::GtEq => Some(ExprKind::BoolLiteral(compare_numeric(left, right, |l, r| l >= r)?)),
        BinOp::Spaceship => Some(ExprKind::IntLiteral(spaceship_numeric(left, right)?)),
        _ => None,
    }
}

pub(super) fn try_fold_null_coalesce(value: &Expr, default: &Expr) -> Option<ExprKind> {
    let value = scalar_value(value)?;
    let default = scalar_value(default)?;
    if matches!(value, ScalarValue::Null) {
        Some(default.into_expr_kind())
    } else {
        Some(value.into_expr_kind())
    }
}

pub(super) fn try_fold_ternary(condition: &Expr, then_expr: &Expr, else_expr: &Expr) -> Option<ExprKind> {
    let condition = scalar_value(condition)?;
    let then_expr = scalar_value(then_expr)?;
    let else_expr = scalar_value(else_expr)?;
    if condition.truthy() {
        Some(then_expr.into_expr_kind())
    } else {
        Some(else_expr.into_expr_kind())
    }
}

pub(super) fn try_fold_short_ternary(value: &Expr, default: &Expr) -> Option<ExprKind> {
    let value = scalar_value(value)?;
    if value.truthy() {
        Some(value.into_expr_kind())
    } else {
        Some(scalar_value(default)?.into_expr_kind())
    }
}

pub(super) fn try_fold_array_access(array: &Expr, index: &Expr) -> Option<ExprKind> {
    match &array.kind {
        ExprKind::ArrayLiteral(items) => try_fold_indexed_array_access(items, index),
        ExprKind::ArrayLiteralAssoc(items) => try_fold_assoc_array_access(items, index),
        _ => None,
    }
}

pub(super) fn try_fold_indexed_array_access(items: &[Expr], index: &Expr) -> Option<ExprKind> {
    let ScalarValue::Int(index) = scalar_value(index)? else {
        return None;
    };
    let index = usize::try_from(index).ok()?;
    let value = items.get(index)?;

    items
        .iter()
        .all(|item| scalar_value(item).is_some())
        .then(|| scalar_value(value).map(ScalarValue::into_expr_kind))
        .flatten()
}

pub(super) fn try_fold_assoc_array_access(items: &[(Expr, Expr)], index: &Expr) -> Option<ExprKind> {
    let index = scalar_value(index)?;
    let mut selected = None;

    for (key, value) in items {
        let key = scalar_value(key)?;
        let value = scalar_value(value)?;
        if key == index {
            selected = Some(value);
        }
    }

    selected.map(ScalarValue::into_expr_kind)
}

pub(super) fn try_fold_cast(target: &CastType, expr: &Expr) -> Option<ExprKind> {
    let value = scalar_value(expr)?;
    match target {
        CastType::Int => try_fold_cast_int(value),
        CastType::Float => try_fold_cast_float(value),
        CastType::String => try_fold_cast_string(value),
        CastType::Bool => Some(ExprKind::BoolLiteral(value.truthy())),
        CastType::Array => None,
    }
}

pub(super) fn try_fold_cast_int(value: ScalarValue) -> Option<ExprKind> {
    match value {
        ScalarValue::Null => Some(ExprKind::IntLiteral(0)),
        ScalarValue::Bool(value) => Some(ExprKind::IntLiteral(i64::from(value))),
        ScalarValue::Int(value) => Some(ExprKind::IntLiteral(value)),
        ScalarValue::Float(value) => truncate_float_to_i64(value).map(ExprKind::IntLiteral),
        ScalarValue::String(value) => parse_string_cast_int(&value).map(ExprKind::IntLiteral),
    }
}

pub(super) fn try_fold_cast_float(value: ScalarValue) -> Option<ExprKind> {
    match value {
        ScalarValue::Null => Some(ExprKind::FloatLiteral(0.0)),
        ScalarValue::Bool(value) => Some(ExprKind::FloatLiteral(if value { 1.0 } else { 0.0 })),
        ScalarValue::Int(value) => Some(ExprKind::FloatLiteral(value as f64)),
        ScalarValue::Float(value) => Some(ExprKind::FloatLiteral(value)),
        ScalarValue::String(value) => parse_string_cast_float(&value).map(ExprKind::FloatLiteral),
    }
}

pub(super) fn try_fold_cast_string(value: ScalarValue) -> Option<ExprKind> {
    match value {
        ScalarValue::Null => Some(ExprKind::StringLiteral(String::new())),
        ScalarValue::Bool(value) => Some(ExprKind::StringLiteral(if value {
            "1".to_string()
        } else {
            String::new()
        })),
        ScalarValue::Int(value) => Some(ExprKind::StringLiteral(value.to_string())),
        ScalarValue::Float(_value) => None,
        ScalarValue::String(value) => Some(ExprKind::StringLiteral(value)),
    }
}

pub(super) fn int_literal(expr: &Expr) -> Option<i64> {
    match expr.kind {
        ExprKind::IntLiteral(value) => Some(value),
        _ => None,
    }
}

pub(super) fn numeric_literal(expr: &Expr) -> Option<f64> {
    match expr.kind {
        ExprKind::IntLiteral(value) => Some(value as f64),
        ExprKind::FloatLiteral(value) => Some(value),
        _ => None,
    }
}

pub(super) fn scalar_value(expr: &Expr) -> Option<ScalarValue> {
    match &expr.kind {
        ExprKind::Null => Some(ScalarValue::Null),
        ExprKind::BoolLiteral(value) => Some(ScalarValue::Bool(*value)),
        ExprKind::IntLiteral(value) => Some(ScalarValue::Int(*value)),
        ExprKind::FloatLiteral(value) => Some(ScalarValue::Float(*value)),
        ExprKind::StringLiteral(value) => Some(ScalarValue::String(value.clone())),
        _ => None,
    }
}

pub(super) fn assigned_scalar_value(expr: &Expr) -> Option<ScalarValue> {
    scalar_value(expr).or_else(|| match &expr.kind {
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_value = assigned_scalar_value(then_expr)?;
            let else_value = assigned_scalar_value(else_expr)?;
            (then_value == else_value).then_some(then_value)
        }
        ExprKind::ShortTernary { value, default } => {
            let value = assigned_scalar_value(value)?;
            if value.truthy() {
                Some(value)
            } else {
                assigned_scalar_value(default)
            }
        }
        ExprKind::Match { arms, default, .. } => {
            let default = default.as_ref()?;
            let default_value = assigned_scalar_value(default)?;
            arms.iter().all(|(_, value)| assigned_scalar_value(value) == Some(default_value.clone()))
                .then_some(default_value)
        }
        _ => None,
    })
}

pub(super) fn strict_eq(left: &Expr, right: &Expr) -> Option<bool> {
    let left = scalar_value(left)?;
    let right = scalar_value(right)?;
    Some(left == right)
}

pub(super) fn loose_eq(left: &Expr, right: &Expr) -> Option<bool> {
    let left = scalar_value(left)?;
    let right = scalar_value(right)?;
    match (&left, &right) {
        (ScalarValue::Null, ScalarValue::Null) => Some(true),
        (ScalarValue::Bool(left), ScalarValue::Bool(right)) => Some(left == right),
        (ScalarValue::String(left), ScalarValue::String(right)) => Some(left == right),
        (ScalarValue::Int(left), ScalarValue::Int(right)) => Some(left == right),
        (ScalarValue::Float(left), ScalarValue::Float(right)) => Some(left == right),
        (ScalarValue::Int(left), ScalarValue::Float(right)) => Some(*left as f64 == *right),
        (ScalarValue::Float(left), ScalarValue::Int(right)) => Some(*left == *right as f64),
        _ => None,
    }
}

pub(super) fn compare_numeric(left: &Expr, right: &Expr, cmp: impl FnOnce(f64, f64) -> bool) -> Option<bool> {
    let left = numeric_literal(left)?;
    let right = numeric_literal(right)?;
    Some(cmp(left, right))
}

pub(super) fn spaceship_numeric(left: &Expr, right: &Expr) -> Option<i64> {
    let left = numeric_literal(left)?;
    let right = numeric_literal(right)?;
    Some(if left < right {
        -1
    } else if left > right {
        1
    } else {
        0
    })
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum ScalarValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

impl ScalarValue {
    pub(super) fn truthy(&self) -> bool {
        match self {
            ScalarValue::Null => false,
            ScalarValue::Bool(value) => *value,
            ScalarValue::Int(value) => *value != 0,
            ScalarValue::Float(value) => *value != 0.0,
            ScalarValue::String(value) => !value.is_empty() && value != "0",
        }
    }

    pub(super) fn into_expr_kind(self) -> ExprKind {
        match self {
            ScalarValue::Null => ExprKind::Null,
            ScalarValue::Bool(value) => ExprKind::BoolLiteral(value),
            ScalarValue::Int(value) => ExprKind::IntLiteral(value),
            ScalarValue::Float(value) => ExprKind::FloatLiteral(value),
            ScalarValue::String(value) => ExprKind::StringLiteral(value),
        }
    }
}

pub(super) fn truncate_float_to_i64(value: f64) -> Option<i64> {
    if !value.is_finite() {
        return None;
    }
    let truncated = value.trunc();
    if truncated < i64::MIN as f64 || truncated > i64::MAX as f64 {
        return None;
    }
    Some(truncated as i64)
}

pub(super) fn parse_string_cast_int(value: &str) -> Option<i64> {
    if let Ok(parsed) = value.parse::<i64>() {
        return Some(parsed);
    }
    if let Ok(parsed) = value.parse::<f64>() {
        return truncate_float_to_i64(parsed);
    }
    if value.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return Some(0);
    }
    None
}

pub(super) fn parse_string_cast_float(value: &str) -> Option<f64> {
    if let Ok(parsed) = value.parse::<f64>() {
        return Some(parsed);
    }
    if value.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return Some(0.0);
    }
    None
}
