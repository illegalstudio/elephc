//! Lowering of PHP magic constants (`__DIR__`, `__FILE__`, `__FUNCTION__`,
//! `__CLASS__`, `__METHOD__`, `__NAMESPACE__`, `__TRAIT__`) to plain string
//! literals before the type checker and codegen run. `__LINE__` is already
//! lowered at parse time (see `parser::expr::prefix`).
//!
//! Two public passes:
//! - [`substitute_file_constants`] resolves `__FILE__` and `__DIR__` against
//!   the canonical path of the file the AST nodes came from. Run once per
//!   source file before inlining (resolver) and once for the main file.
//! - [`substitute_scope_constants`] resolves the scope-dependent constants
//!   (`__FUNCTION__`, `__CLASS__`, `__METHOD__`, `__NAMESPACE__`, `__TRAIT__`)
//!   based on lexical position. Run once after `name_resolver`.

use std::path::Path;

use crate::names::Name;
use crate::parser::ast::{
    CatchClause, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind, MagicConstant, Program,
    Stmt, StmtKind,
};
use crate::span::Span;

/// Replaces `MagicConstant::File` and `MagicConstant::Dir` with string
/// literals derived from `file_path`. Other magic constants are left untouched
/// for the scope pass to resolve later.
pub fn substitute_file_constants(stmts: Vec<Stmt>, file_path: &Path) -> Vec<Stmt> {
    let canonical = file_path
        .canonicalize()
        .unwrap_or_else(|_| file_path.to_path_buf());
    let file = canonical.display().to_string();
    let dir = canonical
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let mut pass = FilePass { file, dir };
    walk_program(stmts, &mut pass)
}

/// Replaces the lexically-scoped magic constants (`__NAMESPACE__`,
/// `__CLASS__`, `__FUNCTION__`, `__METHOD__`, `__TRAIT__`) with string
/// literals derived from the surrounding declaration site.
pub fn substitute_scope_constants(program: Program) -> Program {
    let mut pass = ScopePass {
        scope: Scope::default(),
    };
    walk_program(program, &mut pass)
}

// ---------------------------------------------------------------------------
// Pass trait + generic walker
// ---------------------------------------------------------------------------

trait Pass {
    fn transform_magic(&self, span: Span, mc: MagicConstant) -> ExprKind;

    fn enter_namespace_decl(&mut self, _name: &Option<Name>) {}
    fn enter_namespace_block(&mut self, _name: &Option<Name>) {}
    fn leave_namespace_block(&mut self) {}
    fn enter_function(&mut self, _name: &str) {}
    fn leave_function(&mut self) {}
    fn enter_class(&mut self, _name: &str) {}
    fn leave_class(&mut self) {}
    fn enter_trait(&mut self, _name: &str) {}
    fn leave_trait(&mut self) {}
    fn enter_method(&mut self, _name: &str) {}
    fn leave_method(&mut self) {}
    fn enter_closure(&mut self) {}
    fn leave_closure(&mut self) {}
}

fn walk_program<P: Pass>(stmts: Vec<Stmt>, pass: &mut P) -> Vec<Stmt> {
    stmts.into_iter().map(|s| walk_stmt(s, pass)).collect()
}

fn walk_stmt<P: Pass>(stmt: Stmt, pass: &mut P) -> Stmt {
    let span = stmt.span;
    let kind = match stmt.kind {
        StmtKind::Echo(e) => StmtKind::Echo(walk_expr(e, pass)),
        StmtKind::Throw(e) => StmtKind::Throw(walk_expr(e, pass)),
        StmtKind::ExprStmt(e) => StmtKind::ExprStmt(walk_expr(e, pass)),
        StmtKind::Return(e) => StmtKind::Return(e.map(|x| walk_expr(x, pass))),
        StmtKind::Assign { name, value } => StmtKind::Assign {
            name,
            value: walk_expr(value, pass),
        },
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => StmtKind::TypedAssign {
            type_expr,
            name,
            value: walk_expr(value, pass),
        },
        StmtKind::ConstDecl { name, value } => StmtKind::ConstDecl {
            name,
            value: walk_expr(value, pass),
        },
        StmtKind::ListUnpack { vars, value } => StmtKind::ListUnpack {
            vars,
            value: walk_expr(value, pass),
        },
        StmtKind::StaticVar { name, init } => StmtKind::StaticVar {
            name,
            init: walk_expr(init, pass),
        },
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => StmtKind::ArrayAssign {
            array,
            index: walk_expr(index, pass),
            value: walk_expr(value, pass),
        },
        StmtKind::ArrayPush { array, value } => StmtKind::ArrayPush {
            array,
            value: walk_expr(value, pass),
        },
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => StmtKind::PropertyAssign {
            object: Box::new(walk_expr(*object, pass)),
            property,
            value: walk_expr(value, pass),
        },
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => StmtKind::PropertyArrayPush {
            object: Box::new(walk_expr(*object, pass)),
            property,
            value: walk_expr(value, pass),
        },
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => StmtKind::PropertyArrayAssign {
            object: Box::new(walk_expr(*object, pass)),
            property,
            index: walk_expr(index, pass),
            value: walk_expr(value, pass),
        },
        StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        } => StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value: walk_expr(value, pass),
        },
        StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value,
        } => StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value: walk_expr(value, pass),
        },
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            value,
        } => StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index: walk_expr(index, pass),
            value: walk_expr(value, pass),
        },
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => StmtKind::If {
            condition: walk_expr(condition, pass),
            then_body: walk_program(then_body, pass),
            elseif_clauses: elseif_clauses
                .into_iter()
                .map(|(c, b)| (walk_expr(c, pass), walk_program(b, pass)))
                .collect(),
            else_body: else_body.map(|b| walk_program(b, pass)),
        },
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        } => StmtKind::IfDef {
            symbol,
            then_body: walk_program(then_body, pass),
            else_body: else_body.map(|b| walk_program(b, pass)),
        },
        StmtKind::While { condition, body } => StmtKind::While {
            condition: walk_expr(condition, pass),
            body: walk_program(body, pass),
        },
        StmtKind::DoWhile { body, condition } => StmtKind::DoWhile {
            body: walk_program(body, pass),
            condition: walk_expr(condition, pass),
        },
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => StmtKind::For {
            init: init.map(|s| Box::new(walk_stmt(*s, pass))),
            condition: condition.map(|e| walk_expr(e, pass)),
            update: update.map(|s| Box::new(walk_stmt(*s, pass))),
            body: walk_program(body, pass),
        },
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
        } => StmtKind::Foreach {
            array: walk_expr(array, pass),
            key_var,
            value_var,
            body: walk_program(body, pass),
        },
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => StmtKind::Switch {
            subject: walk_expr(subject, pass),
            cases: cases
                .into_iter()
                .map(|(patterns, body)| {
                    (
                        patterns.into_iter().map(|e| walk_expr(e, pass)).collect(),
                        walk_program(body, pass),
                    )
                })
                .collect(),
            default: default.map(|b| walk_program(b, pass)),
        },
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => StmtKind::Try {
            try_body: walk_program(try_body, pass),
            catches: catches
                .into_iter()
                .map(|c| CatchClause {
                    exception_types: c.exception_types,
                    variable: c.variable,
                    body: walk_program(c.body, pass),
                })
                .collect(),
            finally_body: finally_body.map(|b| walk_program(b, pass)),
        },
        StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            return_type,
            body,
        } => {
            pass.enter_function(&name);
            let new_params = params
                .into_iter()
                .map(|(n, t, default, by_ref)| {
                    (n, t, default.map(|d| walk_expr(d, pass)), by_ref)
                })
                .collect();
            let new_body = walk_program(body, pass);
            pass.leave_function();
            StmtKind::FunctionDecl {
                name,
                params: new_params,
                variadic,
                return_type,
                body: new_body,
            }
        }
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
        } => {
            pass.enter_class(&name);
            let new_properties = properties
                .into_iter()
                .map(|p| walk_class_property(p, pass))
                .collect();
            let new_methods = methods
                .into_iter()
                .map(|m| walk_class_method(m, pass))
                .collect();
            pass.leave_class();
            StmtKind::ClassDecl {
                name,
                extends,
                implements,
                is_abstract,
                is_final,
                is_readonly_class,
                trait_uses,
                properties: new_properties,
                methods: new_methods,
            }
        }
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        } => {
            pass.enter_trait(&name);
            let new_properties = properties
                .into_iter()
                .map(|p| walk_class_property(p, pass))
                .collect();
            let new_methods = methods
                .into_iter()
                .map(|m| walk_class_method(m, pass))
                .collect();
            pass.leave_trait();
            StmtKind::TraitDecl {
                name,
                trait_uses,
                properties: new_properties,
                methods: new_methods,
            }
        }
        StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        } => StmtKind::InterfaceDecl {
            name,
            extends,
            methods: methods
                .into_iter()
                .map(|m| walk_class_method(m, pass))
                .collect(),
        },
        StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
        } => StmtKind::EnumDecl {
            name,
            backing_type,
            cases: cases
                .into_iter()
                .map(|case| EnumCaseDecl {
                    name: case.name,
                    value: case.value.map(|e| walk_expr(e, pass)),
                    span: case.span,
                })
                .collect(),
        },
        StmtKind::NamespaceDecl { name } => {
            pass.enter_namespace_decl(&name);
            StmtKind::NamespaceDecl { name }
        }
        StmtKind::NamespaceBlock { name, body } => {
            pass.enter_namespace_block(&name);
            let new_body = walk_program(body, pass);
            pass.leave_namespace_block();
            StmtKind::NamespaceBlock {
                name,
                body: new_body,
            }
        }
        StmtKind::Include {
            path,
            once,
            required,
        } => StmtKind::Include {
            path: walk_expr(path, pass),
            once,
            required,
        },
        // Statements with no Expr children or only simple data:
        other @ (StmtKind::Break
        | StmtKind::Continue
        | StmtKind::UseDecl { .. }
        | StmtKind::Global { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. }) => other,
    };
    Stmt { kind, span }
}

fn walk_class_property<P: Pass>(prop: ClassProperty, pass: &mut P) -> ClassProperty {
    ClassProperty {
        default: prop.default.map(|e| walk_expr(e, pass)),
        ..prop
    }
}

fn walk_class_method<P: Pass>(method: ClassMethod, pass: &mut P) -> ClassMethod {
    pass.enter_method(&method.name);
    let new_params = method
        .params
        .into_iter()
        .map(|(n, t, default, by_ref)| (n, t, default.map(|d| walk_expr(d, pass)), by_ref))
        .collect();
    let new_body = walk_program(method.body, pass);
    pass.leave_method();
    ClassMethod {
        params: new_params,
        body: new_body,
        ..method
    }
}

fn walk_expr<P: Pass>(expr: Expr, pass: &mut P) -> Expr {
    let span = expr.span;
    let kind = match expr.kind {
        ExprKind::MagicConstant(mc) => pass.transform_magic(span, mc),

        // Leaves with no Expr subtrees:
        kind @ (ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::Variable(_)
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::EnumCase { .. }
        | ExprKind::This
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::FirstClassCallable(_)) => kind,

        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
            left: Box::new(walk_expr(*left, pass)),
            op,
            right: Box::new(walk_expr(*right, pass)),
        },
        ExprKind::Negate(inner) => ExprKind::Negate(Box::new(walk_expr(*inner, pass))),
        ExprKind::Not(inner) => ExprKind::Not(Box::new(walk_expr(*inner, pass))),
        ExprKind::BitNot(inner) => ExprKind::BitNot(Box::new(walk_expr(*inner, pass))),
        ExprKind::Throw(inner) => ExprKind::Throw(Box::new(walk_expr(*inner, pass))),
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(walk_expr(*value, pass)),
            default: Box::new(walk_expr(*default, pass)),
        },
        ExprKind::FunctionCall { name, args } => ExprKind::FunctionCall {
            name,
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::ArrayLiteral(items) => {
            ExprKind::ArrayLiteral(items.into_iter().map(|i| walk_expr(i, pass)).collect())
        }
        ExprKind::ArrayLiteralAssoc(pairs) => ExprKind::ArrayLiteralAssoc(
            pairs
                .into_iter()
                .map(|(k, v)| (walk_expr(k, pass), walk_expr(v, pass)))
                .collect(),
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => ExprKind::Match {
            subject: Box::new(walk_expr(*subject, pass)),
            arms: arms
                .into_iter()
                .map(|(patterns, value)| {
                    (
                        patterns.into_iter().map(|p| walk_expr(p, pass)).collect(),
                        walk_expr(value, pass),
                    )
                })
                .collect(),
            default: default.map(|d| Box::new(walk_expr(*d, pass))),
        },
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(walk_expr(*array, pass)),
            index: Box::new(walk_expr(*index, pass)),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(walk_expr(*condition, pass)),
            then_expr: Box::new(walk_expr(*then_expr, pass)),
            else_expr: Box::new(walk_expr(*else_expr, pass)),
        },
        ExprKind::ShortTernary { value, default } => ExprKind::ShortTernary {
            value: Box::new(walk_expr(*value, pass)),
            default: Box::new(walk_expr(*default, pass)),
        },
        ExprKind::Cast { target, expr: inner } => ExprKind::Cast {
            target,
            expr: Box::new(walk_expr(*inner, pass)),
        },
        ExprKind::Closure {
            params,
            variadic,
            body,
            is_arrow,
            captures,
        } => {
            pass.enter_closure();
            let new_params = params
                .into_iter()
                .map(|(n, t, default, by_ref)| {
                    (n, t, default.map(|d| walk_expr(d, pass)), by_ref)
                })
                .collect();
            let new_body = walk_program(body, pass);
            pass.leave_closure();
            ExprKind::Closure {
                params: new_params,
                variadic,
                body: new_body,
                is_arrow,
                captures,
            }
        }
        ExprKind::NamedArg { name, value } => ExprKind::NamedArg {
            name,
            value: Box::new(walk_expr(*value, pass)),
        },
        ExprKind::Spread(inner) => ExprKind::Spread(Box::new(walk_expr(*inner, pass))),
        ExprKind::ClosureCall { var, args } => ExprKind::ClosureCall {
            var,
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::ExprCall { callee, args } => ExprKind::ExprCall {
            callee: Box::new(walk_expr(*callee, pass)),
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name,
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(walk_expr(*object, pass)),
            property,
        },
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => ExprKind::MethodCall {
            object: Box::new(walk_expr(*object, pass)),
            method,
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => ExprKind::StaticMethodCall {
            receiver,
            method,
            args: args.into_iter().map(|a| walk_expr(a, pass)).collect(),
        },
        ExprKind::PtrCast { target_type, expr: inner } => ExprKind::PtrCast {
            target_type,
            expr: Box::new(walk_expr(*inner, pass)),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type,
            len: Box::new(walk_expr(*len, pass)),
        },
    };
    Expr { kind, span }
}

// ---------------------------------------------------------------------------
// File pass: __FILE__ and __DIR__
// ---------------------------------------------------------------------------

struct FilePass {
    file: String,
    dir: String,
}

impl Pass for FilePass {
    fn transform_magic(&self, _span: Span, mc: MagicConstant) -> ExprKind {
        match mc {
            MagicConstant::File => ExprKind::StringLiteral(self.file.clone()),
            MagicConstant::Dir => ExprKind::StringLiteral(self.dir.clone()),
            other => ExprKind::MagicConstant(other),
        }
    }
}

// ---------------------------------------------------------------------------
// Scope pass: __NAMESPACE__ / __CLASS__ / __FUNCTION__ / __METHOD__ / __TRAIT__
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Scope {
    namespace: Option<String>,
    namespace_stack: Vec<Option<String>>,
    class: Option<String>,
    class_stack: Vec<Option<String>>,
    trait_: Option<String>,
    trait_stack: Vec<Option<String>>,
    function: Option<String>,
    function_stack: Vec<Option<String>>,
    closure_depth: usize,
}

struct ScopePass {
    scope: Scope,
}

impl ScopePass {
    fn fqn_class(&self) -> Option<String> {
        let class = self.scope.class.as_ref()?;
        Some(qualify(self.scope.namespace.as_deref(), class))
    }

    fn fqn_trait(&self) -> Option<String> {
        let trait_name = self.scope.trait_.as_ref()?;
        Some(qualify(self.scope.namespace.as_deref(), trait_name))
    }

    fn fqn_function(&self) -> Option<String> {
        let function = self.scope.function.as_ref()?;
        // For methods inside a class/trait, __FUNCTION__ is the unqualified
        // method name in PHP; for free functions it is namespace-qualified.
        if self.scope.class.is_some() || self.scope.trait_.is_some() {
            Some(function.clone())
        } else {
            Some(qualify(self.scope.namespace.as_deref(), function))
        }
    }
}

impl Pass for ScopePass {
    fn transform_magic(&self, _span: Span, mc: MagicConstant) -> ExprKind {
        let s = &self.scope;
        match mc {
            MagicConstant::Namespace => {
                ExprKind::StringLiteral(s.namespace.clone().unwrap_or_default())
            }
            MagicConstant::Class => {
                ExprKind::StringLiteral(self.fqn_class().unwrap_or_default())
            }
            MagicConstant::Trait => {
                ExprKind::StringLiteral(self.fqn_trait().unwrap_or_default())
            }
            MagicConstant::Function => {
                let name = if s.closure_depth > 0 {
                    "{closure}".to_string()
                } else {
                    self.fqn_function().unwrap_or_default()
                };
                ExprKind::StringLiteral(name)
            }
            MagicConstant::Method => {
                if s.closure_depth > 0 {
                    ExprKind::StringLiteral("{closure}".to_string())
                } else {
                    let name = match (self.fqn_class().or_else(|| self.fqn_trait()), &s.function) {
                        (Some(c), Some(f)) => format!("{}::{}", c, f),
                        (None, Some(f)) => qualify(s.namespace.as_deref(), f),
                        _ => String::new(),
                    };
                    ExprKind::StringLiteral(name)
                }
            }
            // File/Dir are handled by the file pass; if they reach here it
            // means the file pass was skipped — substitute to empty rather
            // than panic, since this is best-effort.
            MagicConstant::File | MagicConstant::Dir => ExprKind::StringLiteral(String::new()),
        }
    }

    fn enter_namespace_decl(&mut self, name: &Option<Name>) {
        self.scope.namespace = Some(namespace_string(name));
    }

    fn enter_namespace_block(&mut self, name: &Option<Name>) {
        self.scope.namespace_stack.push(self.scope.namespace.clone());
        self.scope.namespace = Some(namespace_string(name));
    }

    fn leave_namespace_block(&mut self) {
        if let Some(prev) = self.scope.namespace_stack.pop() {
            self.scope.namespace = prev;
        }
    }

    fn enter_function(&mut self, name: &str) {
        self.scope.function_stack.push(self.scope.function.clone());
        self.scope.function = Some(name.to_string());
    }

    fn leave_function(&mut self) {
        if let Some(prev) = self.scope.function_stack.pop() {
            self.scope.function = prev;
        }
    }

    fn enter_class(&mut self, name: &str) {
        self.scope.class_stack.push(self.scope.class.clone());
        self.scope.class = Some(name.to_string());
    }

    fn leave_class(&mut self) {
        if let Some(prev) = self.scope.class_stack.pop() {
            self.scope.class = prev;
        }
    }

    fn enter_trait(&mut self, name: &str) {
        self.scope.trait_stack.push(self.scope.trait_.clone());
        self.scope.trait_ = Some(name.to_string());
    }

    fn leave_trait(&mut self) {
        if let Some(prev) = self.scope.trait_stack.pop() {
            self.scope.trait_ = prev;
        }
    }

    fn enter_method(&mut self, name: &str) {
        self.enter_function(name);
    }

    fn leave_method(&mut self) {
        self.leave_function();
    }

    fn enter_closure(&mut self) {
        self.scope.closure_depth += 1;
    }

    fn leave_closure(&mut self) {
        self.scope.closure_depth -= 1;
    }
}

fn namespace_string(name: &Option<Name>) -> String {
    name.as_ref().map(Name::as_canonical).unwrap_or_default()
}

fn qualify(namespace: Option<&str>, name: &str) -> String {
    match namespace {
        Some(ns) if !ns.is_empty() => format!("{}\\{}", ns, name),
        _ => name.to_string(),
    }
}
