use super::*;

pub(crate) fn fold_stmt(stmt: Stmt) -> Stmt {
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
            is_final,
            is_readonly_class,
            trait_uses,
            properties,
            methods,
        } => StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_final,
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

pub(crate) fn fold_block(body: Vec<Stmt>) -> Vec<Stmt> {
    body.into_iter().map(fold_stmt).collect()
}
