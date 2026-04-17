use crate::parser::ast::{
    BinOp, CallableTarget, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind, Program,
    Stmt, StmtKind,
};

pub fn fold_constants(program: Program) -> Program {
    program.into_iter().map(fold_stmt).collect()
}

fn fold_stmt(stmt: Stmt) -> Stmt {
    let span = stmt.span;
    let kind = match stmt.kind {
        StmtKind::Echo(expr) => StmtKind::Echo(fold_expr(expr)),
        StmtKind::Assign { name, value } => StmtKind::Assign {
            name,
            value: fold_expr(value),
        },
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => StmtKind::If {
            condition: fold_expr(condition),
            then_body: fold_block(then_body),
            elseif_clauses: elseif_clauses
                .into_iter()
                .map(|(condition, body)| (fold_expr(condition), fold_block(body)))
                .collect(),
            else_body: else_body.map(fold_block),
        },
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        } => StmtKind::IfDef {
            symbol,
            then_body: fold_block(then_body),
            else_body: else_body.map(fold_block),
        },
        StmtKind::While { condition, body } => StmtKind::While {
            condition: fold_expr(condition),
            body: fold_block(body),
        },
        StmtKind::DoWhile { body, condition } => StmtKind::DoWhile {
            body: fold_block(body),
            condition: fold_expr(condition),
        },
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => StmtKind::For {
            init: init.map(|stmt| Box::new(fold_stmt(*stmt))),
            condition: condition.map(fold_expr),
            update: update.map(|stmt| Box::new(fold_stmt(*stmt))),
            body: fold_block(body),
        },
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => StmtKind::ArrayAssign {
            array,
            index: fold_expr(index),
            value: fold_expr(value),
        },
        StmtKind::ArrayPush { array, value } => StmtKind::ArrayPush {
            array,
            value: fold_expr(value),
        },
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => StmtKind::TypedAssign {
            type_expr,
            name,
            value: fold_expr(value),
        },
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
        } => StmtKind::Foreach {
            array: fold_expr(array),
            key_var,
            value_var,
            body: fold_block(body),
        },
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => StmtKind::Switch {
            subject: fold_expr(subject),
            cases: cases
                .into_iter()
                .map(|(exprs, body)| {
                    (
                        exprs.into_iter().map(fold_expr).collect(),
                        fold_block(body),
                    )
                })
                .collect(),
            default: default.map(fold_block),
        },
        StmtKind::Include {
            path,
            once,
            required,
        } => StmtKind::Include {
            path,
            once,
            required,
        },
        StmtKind::Throw(expr) => StmtKind::Throw(fold_expr(expr)),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => StmtKind::Try {
            try_body: fold_block(try_body),
            catches: catches
                .into_iter()
                .map(|catch| crate::parser::ast::CatchClause {
                    exception_types: catch.exception_types,
                    variable: catch.variable,
                    body: fold_block(catch.body),
                })
                .collect(),
            finally_body: finally_body.map(fold_block),
        },
        StmtKind::Break => StmtKind::Break,
        StmtKind::Continue => StmtKind::Continue,
        StmtKind::ExprStmt(expr) => StmtKind::ExprStmt(fold_expr(expr)),
        StmtKind::NamespaceDecl { name } => StmtKind::NamespaceDecl { name },
        StmtKind::NamespaceBlock { name, body } => StmtKind::NamespaceBlock {
            name,
            body: fold_block(body),
        },
        StmtKind::UseDecl { imports } => StmtKind::UseDecl { imports },
        StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            return_type,
            body,
        } => StmtKind::FunctionDecl {
            name,
            params: fold_params(params),
            variadic,
            return_type,
            body: fold_block(body),
        },
        StmtKind::Return(expr) => StmtKind::Return(expr.map(fold_expr)),
        StmtKind::ConstDecl { name, value } => StmtKind::ConstDecl {
            name,
            value: fold_expr(value),
        },
        StmtKind::ListUnpack { vars, value } => StmtKind::ListUnpack {
            vars,
            value: fold_expr(value),
        },
        StmtKind::Global { vars } => StmtKind::Global { vars },
        StmtKind::StaticVar { name, init } => StmtKind::StaticVar {
            name,
            init: fold_expr(init),
        },
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_readonly_class,
            trait_uses,
            properties,
            methods,
        } => StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_readonly_class,
            trait_uses,
            properties: properties.into_iter().map(fold_property).collect(),
            methods: methods.into_iter().map(fold_method).collect(),
        },
        StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
        } => StmtKind::EnumDecl {
            name,
            backing_type,
            cases: cases.into_iter().map(fold_enum_case).collect(),
        },
        StmtKind::PackedClassDecl { name, fields } => StmtKind::PackedClassDecl { name, fields },
        StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        } => StmtKind::InterfaceDecl {
            name,
            extends,
            methods: methods.into_iter().map(fold_method).collect(),
        },
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        } => StmtKind::TraitDecl {
            name,
            trait_uses,
            properties: properties.into_iter().map(fold_property).collect(),
            methods: methods.into_iter().map(fold_method).collect(),
        },
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => StmtKind::PropertyAssign {
            object: Box::new(fold_expr(*object)),
            property,
            value: fold_expr(value),
        },
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => StmtKind::PropertyArrayPush {
            object: Box::new(fold_expr(*object)),
            property,
            value: fold_expr(value),
        },
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => StmtKind::PropertyArrayAssign {
            object: Box::new(fold_expr(*object)),
            property,
            index: fold_expr(index),
            value: fold_expr(value),
        },
        StmtKind::ExternFunctionDecl {
            name,
            params,
            return_type,
            library,
        } => StmtKind::ExternFunctionDecl {
            name,
            params,
            return_type,
            library,
        },
        StmtKind::ExternClassDecl { name, fields } => StmtKind::ExternClassDecl { name, fields },
        StmtKind::ExternGlobalDecl { name, c_type } => {
            StmtKind::ExternGlobalDecl { name, c_type }
        }
    };
    Stmt { kind, span }
}

fn fold_block(body: Vec<Stmt>) -> Vec<Stmt> {
    body.into_iter().map(fold_stmt).collect()
}

fn fold_params(
    params: Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)>,
) -> Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)> {
    params
        .into_iter()
        .map(|(name, type_expr, default, is_ref)| {
            (name, type_expr, default.map(fold_expr), is_ref)
        })
        .collect()
}

fn fold_property(property: ClassProperty) -> ClassProperty {
    ClassProperty {
        name: property.name,
        visibility: property.visibility,
        readonly: property.readonly,
        default: property.default.map(fold_expr),
        span: property.span,
    }
}

fn fold_method(method: ClassMethod) -> ClassMethod {
    ClassMethod {
        name: method.name,
        visibility: method.visibility,
        is_static: method.is_static,
        is_abstract: method.is_abstract,
        has_body: method.has_body,
        params: fold_params(method.params),
        variadic: method.variadic,
        return_type: method.return_type,
        body: fold_block(method.body),
        span: method.span,
    }
}

fn fold_enum_case(case: EnumCaseDecl) -> EnumCaseDecl {
    EnumCaseDecl {
        name: case.name,
        value: case.value.map(fold_expr),
        span: case.span,
    }
}

fn fold_expr(expr: Expr) -> Expr {
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
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(fold_expr(*value)),
            default: Box::new(fold_expr(*default)),
        },
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
        } => ExprKind::Match {
            subject: Box::new(fold_expr(*subject)),
            arms: arms
                .into_iter()
                .map(|(patterns, value)| {
                    (
                        patterns.into_iter().map(fold_expr).collect(),
                        fold_expr(value),
                    )
                })
                .collect(),
            default: default.map(|expr| Box::new(fold_expr(*expr))),
        },
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(fold_expr(*array)),
            index: Box::new(fold_expr(*index)),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(fold_expr(*condition)),
            then_expr: Box::new(fold_expr(*then_expr)),
            else_expr: Box::new(fold_expr(*else_expr)),
        },
        ExprKind::Cast { target, expr } => ExprKind::Cast {
            target,
            expr: Box::new(fold_expr(*expr)),
        },
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
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => ExprKind::MethodCall {
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
    };
    Expr { kind, span }
}

fn fold_callable_target(target: CallableTarget) -> CallableTarget {
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

fn try_fold_negate(expr: &Expr) -> Option<ExprKind> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => value.checked_neg().map(ExprKind::IntLiteral),
        ExprKind::FloatLiteral(value) => Some(ExprKind::FloatLiteral(-value)),
        _ => None,
    }
}

fn try_fold_not(expr: &Expr) -> Option<ExprKind> {
    match &expr.kind {
        ExprKind::BoolLiteral(value) => Some(ExprKind::BoolLiteral(!value)),
        _ => None,
    }
}

fn try_fold_bit_not(expr: &Expr) -> Option<ExprKind> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(ExprKind::IntLiteral(!value)),
        _ => None,
    }
}

fn try_fold_binary_op(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
    match op {
        BinOp::Concat => try_fold_concat(left, right),
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow => {
            try_fold_numeric_binop(op, left, right)
        }
        BinOp::Mod => try_fold_int_mod(left, right),
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight => {
            try_fold_bitwise_binop(op, left, right)
        }
        _ => None,
    }
}

fn try_fold_concat(left: &Expr, right: &Expr) -> Option<ExprKind> {
    let ExprKind::StringLiteral(left) = &left.kind else {
        return None;
    };
    let ExprKind::StringLiteral(right) = &right.kind else {
        return None;
    };
    Some(ExprKind::StringLiteral(format!("{left}{right}")))
}

fn try_fold_numeric_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
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

fn try_fold_int_numeric_binop(op: &BinOp, left: i64, right: i64) -> Option<ExprKind> {
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

fn try_fold_int_mod(left: &Expr, right: &Expr) -> Option<ExprKind> {
    let (left, right) = (int_literal(left)?, int_literal(right)?);
    if right == 0 {
        None
    } else {
        Some(ExprKind::IntLiteral(left % right))
    }
}

fn try_fold_bitwise_binop(op: &BinOp, left: &Expr, right: &Expr) -> Option<ExprKind> {
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

fn int_literal(expr: &Expr) -> Option<i64> {
    match expr.kind {
        ExprKind::IntLiteral(value) => Some(value),
        _ => None,
    }
}

fn numeric_literal(expr: &Expr) -> Option<f64> {
    match expr.kind {
        ExprKind::IntLiteral(value) => Some(value as f64),
        ExprKind::FloatLiteral(value) => Some(value),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::{ClassProperty, Visibility};
    use crate::span::Span;

    #[test]
    fn test_fold_nested_integer_arithmetic() {
        let program = vec![Stmt::new(
            StmtKind::Echo(Expr::new(
                ExprKind::BinaryOp {
                    left: Box::new(Expr::new(
                        ExprKind::BinaryOp {
                            left: Box::new(Expr::int_lit(2)),
                            op: BinOp::Add,
                            right: Box::new(Expr::int_lit(3)),
                        },
                        Span::dummy(),
                    )),
                    op: BinOp::Mul,
                    right: Box::new(Expr::int_lit(4)),
                },
                Span::dummy(),
            )),
            Span::dummy(),
        )];

        let folded = fold_constants(program);

        assert_eq!(folded, vec![Stmt::echo(Expr::int_lit(20))]);
    }

    #[test]
    fn test_fold_constant_pow_to_float_literal() {
        let program = vec![Stmt::echo(Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::int_lit(2)),
                op: BinOp::Pow,
                right: Box::new(Expr::int_lit(3)),
            },
            Span::dummy(),
        ))];

        let folded = fold_constants(program);

        assert_eq!(
            folded,
            vec![Stmt::echo(Expr::new(
                ExprKind::FloatLiteral(8.0),
                Span::dummy(),
            ))]
        );
    }

    #[test]
    fn test_skip_division_by_zero_fold() {
        let expr = Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::int_lit(5)),
                op: BinOp::Div,
                right: Box::new(Expr::int_lit(0)),
            },
            Span::dummy(),
        );

        let folded = fold_constants(vec![Stmt::echo(expr.clone())]);

        assert_eq!(folded, vec![Stmt::echo(expr)]);
    }

    #[test]
    fn test_fold_string_concat_and_property_default() {
        let property = ClassProperty {
            name: "label".to_string(),
            visibility: Visibility::Public,
            readonly: false,
            default: Some(Expr::new(
                ExprKind::BinaryOp {
                    left: Box::new(Expr::string_lit("hello ")),
                    op: BinOp::Concat,
                    right: Box::new(Expr::string_lit("world")),
                },
                Span::dummy(),
            )),
            span: Span::dummy(),
        };

        let folded = fold_constants(vec![Stmt::new(
            StmtKind::ClassDecl {
                name: "Greeter".to_string(),
                extends: None,
                implements: Vec::new(),
                is_abstract: false,
                is_readonly_class: false,
                trait_uses: Vec::new(),
                properties: vec![property],
                methods: Vec::new(),
            },
            Span::dummy(),
        )]);

        let StmtKind::ClassDecl { properties, .. } = &folded[0].kind else {
            panic!("expected class declaration");
        };
        assert_eq!(
            properties[0].default,
            Some(Expr::string_lit("hello world"))
        );
    }
}
