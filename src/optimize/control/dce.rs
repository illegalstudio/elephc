use super::*;

pub(crate) fn dce_block(body: Vec<Stmt>) -> Vec<Stmt> {
    let mut eliminated = Vec::new();
    for stmt in body {
        let dce_stmt = dce_stmt(stmt);
        let stops_here = dce_stmt
            .last()
            .is_some_and(|stmt| !matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough));
        eliminated.extend(dce_stmt);
        if stops_here {
            break;
        }
    }
    eliminated
}

pub(crate) fn dce_stmt(stmt: Stmt) -> Vec<Stmt> {
    let span = stmt.span;
    match stmt.kind {
        StmtKind::Echo(expr) => vec![Stmt {
            kind: StmtKind::Echo(prune_expr(expr)),
            span,
        }],
        StmtKind::Assign { name, value } => vec![Stmt {
            kind: StmtKind::Assign {
                name,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::TypedAssign {
            name,
            type_expr,
            value,
        } => vec![Stmt {
            kind: StmtKind::TypedAssign {
                name,
                type_expr,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => vec![Stmt {
            kind: StmtKind::PropertyAssign {
                object: Box::new(prune_expr(*object)),
                property,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => vec![Stmt {
            kind: StmtKind::PropertyArrayAssign {
                object: Box::new(prune_expr(*object)),
                property,
                index: prune_expr(index),
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => vec![Stmt {
            kind: StmtKind::PropertyArrayPush {
                object: Box::new(prune_expr(*object)),
                property,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::ArrayAssign { array, index, value } => vec![Stmt {
            kind: StmtKind::ArrayAssign {
                array,
                index: prune_expr(index),
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::ArrayPush { array, value } => vec![Stmt {
            kind: StmtKind::ArrayPush {
                array,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::ListUnpack { vars, value } => vec![Stmt {
            kind: StmtKind::ListUnpack {
                vars,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::StaticVar { name, init } => vec![Stmt {
            kind: StmtKind::StaticVar {
                name,
                init: prune_expr(init),
            },
            span,
        }],
        StmtKind::ConstDecl { name, value } => vec![Stmt {
            kind: StmtKind::ConstDecl {
                name,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => vec![Stmt {
            kind: StmtKind::If {
                condition: prune_expr(condition),
                then_body: dce_block(then_body),
                elseif_clauses: elseif_clauses
                    .into_iter()
                    .map(|(condition, body)| (prune_expr(condition), dce_block(body)))
                    .collect(),
                else_body: normalize_optional_block(else_body.map(dce_block)),
            },
            span,
        }],
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        } => {
            let then_body = dce_block(then_body);
            let else_body = normalize_optional_block(else_body.map(dce_block));
            if then_body.is_empty() && else_body.is_none() {
                Vec::new()
            } else {
                vec![Stmt {
                    kind: StmtKind::IfDef {
                        symbol,
                        then_body,
                        else_body,
                    },
                    span,
                }]
            }
        }
        StmtKind::While { condition, body } => vec![Stmt {
            kind: StmtKind::While {
                condition: prune_expr(condition),
                body: dce_block(body),
            },
            span,
        }],
        StmtKind::DoWhile { body, condition } => vec![Stmt {
            kind: StmtKind::DoWhile {
                body: dce_block(body),
                condition: prune_expr(condition),
            },
            span,
        }],
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => vec![Stmt {
            kind: StmtKind::For {
                init: init.and_then(|stmt| dce_stmt(*stmt).into_iter().next().map(Box::new)),
                condition: condition.map(prune_expr),
                update: update.and_then(|stmt| dce_stmt(*stmt).into_iter().next().map(Box::new)),
                body: dce_block(body),
            },
            span,
        }],
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
        } => vec![Stmt {
            kind: StmtKind::Foreach {
                array: prune_expr(array),
                key_var,
                value_var,
                body: dce_block(body),
            },
            span,
        }],
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => vec![Stmt {
            kind: StmtKind::Switch {
                subject: prune_expr(subject),
                cases: cases
                    .into_iter()
                    .map(|(patterns, body)| {
                        (
                            patterns.into_iter().map(prune_expr).collect(),
                            dce_block(body),
                        )
                    })
                    .collect(),
                default: normalize_optional_block(default.map(dce_block)),
            },
            span,
        }],
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => vec![Stmt {
            kind: StmtKind::Try {
                try_body: dce_block(try_body),
                catches: catches
                    .into_iter()
                    .map(|catch| crate::parser::ast::CatchClause {
                        exception_types: catch.exception_types,
                        variable: catch.variable,
                        body: dce_block(catch.body),
                    })
                    .collect(),
                finally_body: normalize_optional_block(finally_body.map(dce_block)),
            },
            span,
        }],
        StmtKind::NamespaceBlock { name, body } => vec![Stmt {
            kind: StmtKind::NamespaceBlock {
                name,
                body: dce_block(body),
            },
            span,
        }],
        StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            return_type,
            body,
        } => vec![Stmt {
            kind: StmtKind::FunctionDecl {
                name,
                params,
                variadic,
                return_type,
                body: dce_block(body),
            },
            span,
        }],
        StmtKind::Return(expr) => vec![Stmt {
            kind: StmtKind::Return(expr.map(prune_expr)),
            span,
        }],
        StmtKind::Throw(expr) => vec![Stmt {
            kind: StmtKind::Throw(prune_expr(expr)),
            span,
        }],
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_readonly_class,
            trait_uses,
            properties,
            methods,
        } => {
            let parent_name = extends.as_ref().map(|parent| parent.as_str().to_string());
            let methods = methods
                .into_iter()
                .map(|method| dce_method(method, &name, parent_name.as_deref()))
                .collect();
            vec![Stmt {
                kind: StmtKind::ClassDecl {
                    name,
                    extends,
                    implements,
                    is_abstract,
                    is_readonly_class,
                    trait_uses,
                    properties,
                    methods,
                },
                span,
            }]
        }
        StmtKind::ExprStmt(expr) => {
            let expr = prune_expr(expr);
            if expr_has_side_effects(&expr) {
                vec![Stmt {
                    kind: StmtKind::ExprStmt(expr),
                    span,
                }]
            } else {
                Vec::new()
            }
        }
        StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
        } => vec![Stmt {
            kind: StmtKind::EnumDecl {
                name,
                backing_type,
                cases,
            },
            span,
        }],
        StmtKind::PackedClassDecl { name, fields } => vec![Stmt {
            kind: StmtKind::PackedClassDecl { name, fields },
            span,
        }],
        StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        } => vec![Stmt {
            kind: StmtKind::InterfaceDecl {
                name,
                extends,
                methods: methods
                    .into_iter()
                    .map(dce_method_without_context)
                    .collect(),
            },
            span,
        }],
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        } => vec![Stmt {
            kind: StmtKind::TraitDecl {
                name,
                trait_uses,
                properties,
                methods: methods
                    .into_iter()
                    .map(dce_method_without_context)
                    .collect(),
            },
            span,
        }],
        kind => vec![Stmt { kind, span }],
    }
}

pub(crate) fn dce_method(method: ClassMethod, class_name: &str, parent_name: Option<&str>) -> ClassMethod {
    let context = ClassEffectContext {
        class_name: class_name.to_string(),
        parent_name: parent_name.map(str::to_string),
    };
    ClassMethod {
        body: with_class_effect_context(Some(context), || dce_block(method.body)),
        ..method
    }
}

pub(crate) fn dce_method_without_context(method: ClassMethod) -> ClassMethod {
    ClassMethod {
        body: with_class_effect_context(None, || dce_block(method.body)),
        ..method
    }
}
