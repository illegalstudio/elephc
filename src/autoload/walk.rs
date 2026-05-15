//! Purpose:
//! Collects declared and referenced fully-qualified class-like names from the AST.
//! Gives the autoload pass the missing symbols it can try to resolve from disk.
//!
//! Called from:
//! - `crate::autoload::run()`
//!
//! Key details:
//! - Literal `class_exists(..., true)` shapes are treated as compile-time autoload demands.
//! - Dynamic autoload flags are not guessed because the checker rejects them in AOT mode.

use std::collections::HashSet;

use crate::parser::ast::{
    CallableTarget, CatchClause, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, Program,
    StaticReceiver, Stmt, StmtKind, TraitUse, TypeExpr,
};

pub(super) fn collect_declared_fqns(program: &Program) -> HashSet<String> {
    let mut out = HashSet::new();
    for stmt in program {
        collect_declared_in_stmt(stmt, &mut out);
    }
    out
}

fn collect_declared_in_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
    match &stmt.kind {
        StmtKind::ClassDecl { name, .. }
        | StmtKind::InterfaceDecl { name, .. }
        | StmtKind::TraitDecl { name, .. }
        | StmtKind::EnumDecl { name, .. }
        | StmtKind::PackedClassDecl { name, .. } => {
            out.insert(name.trim_start_matches('\\').to_string());
        }
        StmtKind::NamespaceBlock { body, .. } => {
            for inner in body {
                collect_declared_in_stmt(inner, out);
            }
        }
        _ => {}
    }
}

pub(super) fn collect_reference_points(program: &Program) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (stmt_idx, stmt) in program.iter().enumerate() {
        let mut refs = HashSet::new();
        collect_refs_stmt(stmt, &mut refs);
        let mut refs: Vec<String> = refs.into_iter().collect();
        refs.sort();
        out.extend(refs.into_iter().map(|fqn| (stmt_idx, fqn)));
    }
    out
}

fn collect_refs_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
    match &stmt.kind {
        StmtKind::ClassDecl {
            extends,
            implements,
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            if let Some(parent) = extends {
                push_name(parent, out);
            }
            for iface in implements {
                push_name(iface, out);
            }
            for tu in trait_uses {
                collect_trait_use(tu, out);
            }
            for prop in properties {
                collect_property(prop, out);
            }
            for method in methods {
                collect_method(method, out);
            }
            for constant in constants {
                collect_class_const(constant, out);
            }
        }
        StmtKind::InterfaceDecl {
            extends,
            methods,
            constants,
            ..
        } => {
            for parent in extends {
                push_name(parent, out);
            }
            for method in methods {
                collect_method(method, out);
            }
            for constant in constants {
                collect_class_const(constant, out);
            }
        }
        StmtKind::TraitDecl {
            trait_uses,
            properties,
            methods,
            constants,
            ..
        } => {
            for tu in trait_uses {
                collect_trait_use(tu, out);
            }
            for prop in properties {
                collect_property(prop, out);
            }
            for method in methods {
                collect_method(method, out);
            }
            for constant in constants {
                collect_class_const(constant, out);
            }
        }
        StmtKind::EnumDecl {
            backing_type,
            cases,
            ..
        } => {
            if let Some(t) = backing_type {
                collect_type_expr(t, out);
            }
            for case in cases {
                if let Some(value) = &case.value {
                    collect_refs_expr(value, out);
                }
            }
        }
        StmtKind::PackedClassDecl { fields, .. } => {
            for field in fields {
                collect_type_expr(&field.type_expr, out);
            }
        }
        StmtKind::FunctionDecl {
            params,
            return_type,
            body,
            ..
        } => {
            for (_, type_expr, default, _) in params {
                if let Some(t) = type_expr {
                    collect_type_expr(t, out);
                }
                if let Some(d) = default {
                    collect_refs_expr(d, out);
                }
            }
            if let Some(rt) = return_type {
                collect_type_expr(rt, out);
            }
            for s in body {
                collect_refs_stmt(s, out);
            }
        }
        StmtKind::NamespaceBlock { body, .. } => {
            for inner in body {
                collect_refs_stmt(inner, out);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                collect_refs_stmt(s, out);
            }
            if let Some(body) = else_body {
                for s in body {
                    collect_refs_stmt(s, out);
                }
            }
        }
        StmtKind::Assign { value, .. } => collect_refs_expr(value, out),
        StmtKind::ExprStmt(e) => collect_refs_expr(e, out),
        StmtKind::Return(Some(e)) => collect_refs_expr(e, out),
        StmtKind::Echo(e) => collect_refs_expr(e, out),
        StmtKind::Include { path, .. } => collect_refs_expr(path, out),
        StmtKind::IncludeOnceGuard { body, .. } => {
            for s in body {
                collect_refs_stmt(s, out);
            }
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_refs_expr(condition, out);
            for s in then_body {
                collect_refs_stmt(s, out);
            }
            for (cond, body) in elseif_clauses {
                collect_refs_expr(cond, out);
                for s in body {
                    collect_refs_stmt(s, out);
                }
            }
            if let Some(body) = else_body {
                for s in body {
                    collect_refs_stmt(s, out);
                }
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { body, condition } => {
            collect_refs_expr(condition, out);
            for s in body {
                collect_refs_stmt(s, out);
            }
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(s) = init {
                collect_refs_stmt(s, out);
            }
            if let Some(c) = condition {
                collect_refs_expr(c, out);
            }
            if let Some(s) = update {
                collect_refs_stmt(s, out);
            }
            for s in body {
                collect_refs_stmt(s, out);
            }
        }
        StmtKind::Foreach {
            array, body, ..
        } => {
            collect_refs_expr(array, out);
            for s in body {
                collect_refs_stmt(s, out);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            for s in try_body {
                collect_refs_stmt(s, out);
            }
            for catch in catches {
                collect_refs_catch(catch, out);
            }
            if let Some(f) = finally_body {
                for s in f {
                    collect_refs_stmt(s, out);
                }
            }
        }
        StmtKind::Switch { subject, cases, default } => {
            collect_refs_expr(subject, out);
            for (values, body) in cases {
                for v in values {
                    collect_refs_expr(v, out);
                }
                for s in body {
                    collect_refs_stmt(s, out);
                }
            }
            if let Some(default_body) = default {
                for s in default_body {
                    collect_refs_stmt(s, out);
                }
            }
        }
        StmtKind::ConstDecl { value, .. } | StmtKind::StaticVar { init: value, .. } => {
            collect_refs_expr(value, out);
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            collect_refs_expr(object, out);
            collect_refs_expr(value, out);
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            collect_refs_expr(object, out);
            collect_refs_expr(index, out);
            collect_refs_expr(value, out);
        }
        StmtKind::StaticPropertyAssign {
            receiver, value, ..
        }
        | StmtKind::StaticPropertyArrayPush {
            receiver, value, ..
        } => {
            collect_static_receiver(receiver, out);
            collect_refs_expr(value, out);
        }
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            index,
            value,
            ..
        } => {
            collect_static_receiver(receiver, out);
            collect_refs_expr(index, out);
            collect_refs_expr(value, out);
        }
        StmtKind::ArrayAssign { value, index, .. } => {
            collect_refs_expr(value, out);
            collect_refs_expr(index, out);
        }
        StmtKind::ArrayPush { value, .. } => collect_refs_expr(value, out),
        StmtKind::ListUnpack { value, .. } => collect_refs_expr(value, out),
        StmtKind::TypedAssign { type_expr, value, .. } => {
            collect_type_expr(type_expr, out);
            collect_refs_expr(value, out);
        }
        StmtKind::Throw(e) => collect_refs_expr(e, out),
        StmtKind::Synthetic(stmts) => {
            for s in stmts {
                collect_refs_stmt(s, out);
            }
        }
        _ => {}
    }
}

fn collect_refs_catch(catch: &CatchClause, out: &mut HashSet<String>) {
    for ty in &catch.exception_types {
        push_name(ty, out);
    }
    for s in &catch.body {
        collect_refs_stmt(s, out);
    }
}

fn collect_method(method: &ClassMethod, out: &mut HashSet<String>) {
    for (_, type_expr, default, _) in &method.params {
        if let Some(t) = type_expr {
            collect_type_expr(t, out);
        }
        if let Some(d) = default {
            collect_refs_expr(d, out);
        }
    }
    if let Some(rt) = &method.return_type {
        collect_type_expr(rt, out);
    }
    for s in &method.body {
        collect_refs_stmt(s, out);
    }
}

fn collect_property(prop: &ClassProperty, out: &mut HashSet<String>) {
    if let Some(t) = &prop.type_expr {
        collect_type_expr(t, out);
    }
    if let Some(d) = &prop.default {
        collect_refs_expr(d, out);
    }
}

fn collect_class_const(constant: &ClassConst, out: &mut HashSet<String>) {
    collect_refs_expr(&constant.value, out);
}

fn collect_trait_use(trait_use: &TraitUse, out: &mut HashSet<String>) {
    for name in &trait_use.trait_names {
        push_name(name, out);
    }
}

fn collect_type_expr(ty: &TypeExpr, out: &mut HashSet<String>) {
    match ty {
        TypeExpr::Named(name) => push_name(name, out),
        TypeExpr::Nullable(inner) => collect_type_expr(inner, out),
        TypeExpr::Union(parts) => {
            for p in parts {
                collect_type_expr(p, out);
            }
        }
        TypeExpr::Buffer(inner) => collect_type_expr(inner, out),
        TypeExpr::Ptr(Some(name)) => push_name(name, out),
        _ => {}
    }
}

fn collect_refs_expr(expr: &Expr, out: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::NewObject { class_name, args } => {
            push_name(class_name, out);
            for a in args {
                collect_refs_expr(a, out);
            }
        }
        ExprKind::InstanceOf { value, target } => {
            collect_refs_expr(value, out);
            match target {
                crate::parser::ast::InstanceOfTarget::Name(name) => push_name(name, out),
                crate::parser::ast::InstanceOfTarget::Expr(inner) => collect_refs_expr(inner, out),
            }
        }
        ExprKind::ClassConstant { receiver, .. }
        | ExprKind::ScopedConstantAccess { receiver, .. }
        | ExprKind::StaticPropertyAccess { receiver, .. } => {
            collect_static_receiver(receiver, out);
        }
        ExprKind::StaticMethodCall {
            receiver, args, ..
        } => {
            collect_static_receiver(receiver, out);
            for a in args {
                collect_refs_expr(a, out);
            }
        }
        ExprKind::Cast { expr: inner, .. } => collect_refs_expr(inner, out),
        ExprKind::PtrCast { expr: inner, .. } => collect_refs_expr(inner, out),
        ExprKind::BinaryOp { left, right, .. } => {
            collect_refs_expr(left, out);
            collect_refs_expr(right, out);
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner) => collect_refs_expr(inner, out),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            collect_refs_expr(value, out);
            collect_refs_expr(default, out);
        }
        ExprKind::Pipe { value, callable } => {
            collect_refs_expr(value, out);
            collect_refs_expr(callable, out);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_refs_expr(condition, out);
            collect_refs_expr(then_expr, out);
            collect_refs_expr(else_expr, out);
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_refs_expr(subject, out);
            for (patterns, value) in arms {
                for p in patterns {
                    collect_refs_expr(p, out);
                }
                collect_refs_expr(value, out);
            }
            if let Some(d) = default {
                collect_refs_expr(d, out);
            }
        }
        ExprKind::Assignment { target, value, prelude, .. } => {
            collect_refs_expr(target, out);
            collect_refs_expr(value, out);
            for s in prelude {
                collect_refs_stmt(s, out);
            }
        }
        ExprKind::FunctionCall { name, args } => {
            // Detect compile-time demands for a literal class name. The
            // autoload pass picks these up like any other class reference.
            let canonical = name.as_canonical();
            let trimmed = canonical.trim_start_matches('\\');
            match trimmed {
                "spl_autoload_call" | "spl_autoload" => {
                    push_literal_fqn(args.first(), out);
                }
                "class_exists" | "interface_exists" | "trait_exists" | "enum_exists" => {
                    // The autoload-controlling second arg only triggers when
                    // omitted or when it is a literal truthy value. A dynamic
                    // expression must not be guessed as true at AOT time.
                    let triggers_autoload = match args.get(1).map(|arg| &arg.kind) {
                        None => true,
                        Some(ExprKind::BoolLiteral(b)) => *b,
                        Some(ExprKind::IntLiteral(n)) => *n != 0,
                        Some(_) => false,
                    };
                    if triggers_autoload {
                        push_literal_fqn(args.first(), out);
                    }
                }
                _ => {}
            }
            for a in args {
                collect_refs_expr(a, out);
            }
        }
        ExprKind::ClosureCall { args, .. } => {
            for a in args {
                collect_refs_expr(a, out);
            }
        }
        ExprKind::ExprCall { callee, args } => {
            collect_refs_expr(callee, out);
            for a in args {
                collect_refs_expr(a, out);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_refs_expr(array, out);
            collect_refs_expr(index, out);
        }
        ExprKind::ArrayLiteral(items) => {
            for i in items {
                collect_refs_expr(i, out);
            }
        }
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (k, v) in pairs {
                collect_refs_expr(k, out);
                collect_refs_expr(v, out);
            }
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => collect_refs_expr(object, out),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            collect_refs_expr(object, out);
            for a in args {
                collect_refs_expr(a, out);
            }
        }
        ExprKind::NewScopedObject { receiver, args } => {
            collect_static_receiver(receiver, out);
            for a in args {
                collect_refs_expr(a, out);
            }
        }
        ExprKind::NamedArg { value, .. } => collect_refs_expr(value, out),
        ExprKind::Closure {
            params,
            return_type,
            body,
            ..
        } => {
            for (_, type_expr, default, _) in params {
                if let Some(t) = type_expr {
                    collect_type_expr(t, out);
                }
                if let Some(d) = default {
                    collect_refs_expr(d, out);
                }
            }
            if let Some(rt) = return_type {
                collect_type_expr(rt, out);
            }
            for s in body {
                collect_refs_stmt(s, out);
            }
        }
        ExprKind::FirstClassCallable(target) => collect_callable_target(target, out),
        ExprKind::BufferNew { element_type, len } => {
            collect_type_expr(element_type, out);
            collect_refs_expr(len, out);
        }
        ExprKind::Yield { key, value } => {
            if let Some(k) = key {
                collect_refs_expr(k, out);
            }
            if let Some(v) = value {
                collect_refs_expr(v, out);
            }
        }
        ExprKind::YieldFrom(inner) => collect_refs_expr(inner, out),
        _ => {}
    }
}

fn collect_static_receiver(receiver: &StaticReceiver, out: &mut HashSet<String>) {
    if let StaticReceiver::Named(name) = receiver {
        push_name(name, out);
    }
}

fn collect_callable_target(target: &CallableTarget, out: &mut HashSet<String>) {
    match target {
        CallableTarget::StaticMethod { receiver, .. } => collect_static_receiver(receiver, out),
        CallableTarget::Method { object, .. } => collect_refs_expr(object, out),
        CallableTarget::Function(_) => {}
    }
}

fn push_name(name: &crate::names::Name, out: &mut HashSet<String>) {
    let canonical = name.as_canonical();
    let trimmed = canonical.trim_start_matches('\\');
    if !trimmed.is_empty() {
        out.insert(trimmed.to_string());
    }
}

fn push_literal_fqn(arg: Option<&crate::parser::ast::Expr>, out: &mut HashSet<String>) {
    let Some(arg) = arg else { return };
    let ExprKind::StringLiteral(name) = &arg.kind else {
        return;
    };
    let cleaned = name.trim_start_matches('\\').to_string();
    if !cleaned.is_empty() {
        out.insert(cleaned);
    }
}
