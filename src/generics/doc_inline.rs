//! Purpose:
//! Replaces each call of a PHPDoc method template whose body is `return $param;` by its argument,
//! once the check is over.
//!
//! Called from:
//! - `crate::generics::monomorphize`, after the last check and before template removal.
//!
//! Key details:
//! - The checker proved the replacement (`Checker::doc_inline_sites`): the body returns its one
//!   parameter and nothing else, no subclass answers the call differently, and the receiver is a
//!   variable that cannot be null. The call then IS its argument, so the value keeps its own type
//!   and representation and no call, box or conversion is left. It is no copy of the method and no
//!   new check: the checker already typed the call by the argument's type.
//! - EIR lowers class and enum methods from the checker's retained method bodies, so those are
//!   rewritten too, under the scope the checker keyed the sites by (`SiteScope`).

use crate::fast_hash::FastMap as HashMap;

use crate::generics::site_scope::SiteScope;
use crate::magic_constants::walker::{walk_program, Pass};
use crate::parser::ast::{Expr, ExprKind, MagicConstant, Program};
use crate::span::Span;
use crate::types::CheckResult;

/// Rewrites every recorded identity call in the program and in the checker's method bodies.
pub(super) fn inline(program: Program, check_result: &mut CheckResult) -> Program {
    if check_result.doc_inline_sites.is_empty() {
        return program;
    }
    let sites = check_result
        .doc_inline_sites
        .keys()
        .map(|site| (site.clone(), String::new()))
        .collect();
    let mut pass = Inline { sites, scope: SiteScope::default() };
    let program = walk_program(program, &mut pass);
    for (class, info) in check_result.classes.iter_mut() {
        for method in &mut info.method_decls {
            pass.scope.enter_class(class);
            pass.scope.enter_method(&method.name);
            method.body = walk_program(std::mem::take(&mut method.body), &mut pass);
            pass.scope.leave_body();
            pass.scope.leave_class();
        }
    }
    program
}

/// Walks a body under the scope the checker keyed its sites by.
struct Inline {
    sites: HashMap<(String, Span), String>,
    scope: SiteScope,
}

impl Pass for Inline {
    /// Required by the trait; this pass rewrites no magic constant.
    fn transform_magic(&self, _span: Span, mc: MagicConstant) -> ExprKind {
        ExprKind::MagicConstant(mc)
    }

    /// Replaces a recorded identity call by its only argument.
    fn transform_expr(&self, expr: Expr) -> Expr {
        let recorded = matches!(
            expr.kind,
            ExprKind::MethodCall { .. } | ExprKind::StaticMethodCall { .. }
        ) && self.scope.resolve(&self.sites, expr.span).is_some();
        if !recorded {
            return expr;
        }
        match expr.kind {
            ExprKind::MethodCall { mut args, .. } | ExprKind::StaticMethodCall { mut args, .. }
                if args.len() == 1 =>
            {
                args.remove(0)
            }
            kind => Expr { kind, span: expr.span },
        }
    }

    /// Tracks the lexical class used to identify call sites.
    fn enter_class(&mut self, name: &str) {
        self.scope.enter_class(name);
    }

    /// Restores the enclosing lexical class.
    fn leave_class(&mut self) {
        self.scope.leave_class();
    }

    /// Tracks the trait scope used when its methods are flattened into classes.
    fn enter_trait(&mut self, name: &str) {
        self.scope.enter_trait(name);
    }

    /// Restores the enclosing scope after a trait.
    fn leave_trait(&mut self) {
        self.scope.leave_trait();
    }

    /// Tracks the enclosing function for the sites' scope key.
    fn enter_function(&mut self, name: &str) {
        self.scope.enter_function(name);
    }

    /// Restores the enclosing body scope after a function.
    fn leave_function(&mut self) {
        self.scope.leave_body();
    }

    /// Tracks the enclosing method for the sites' scope key.
    fn enter_method(&mut self, name: &str, _type_params: &[crate::parser::ast::TypeParam]) {
        self.scope.enter_method(name);
    }

    /// Restores the enclosing body scope after a method.
    fn leave_method(&mut self) {
        self.scope.leave_body();
    }

    /// Tracks the nested closure scope for the sites' scope key.
    fn enter_closure(&mut self, span: Span) {
        self.scope.enter_closure(span);
    }

    /// Restores the enclosing body scope after a closure.
    fn leave_closure(&mut self) {
        self.scope.leave_body();
    }
}
