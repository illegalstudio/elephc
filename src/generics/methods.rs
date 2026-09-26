//! Purpose:
//! Lifts every generic METHOD out of the program, and splices back the instantiations the
//! checker asked for, renaming each call site to the one it selected.
//!
//! Called from:
//! - `crate::generics::monomorphize`, once per fixpoint round.
//!
//! Key details:
//! - A generic method's type parameters are bound by its CALL, not by its class. `Box<T>` binds
//!   `T` when the class is instantiated and `U` when `map<U>` is called, so the two halves run
//!   at different times and a method template survives class instantiation untouched.
//! - The call has to be RENAMED, because an instantiation is an ordinary method with its own
//!   name: `$r->pickOr(1, 0)` becomes `$r->pickOr<int>(1, 0)`. Nothing downstream could work
//!   that out, since only the checker knows the argument types.
//! - An instantiated method never takes a vtable slot. Two instantiations of one class must keep
//!   identical method sets or their slot numbering diverges, and a variance widening then takes
//!   the slot from one and indexes the other — the failure `reachability/reconcile.rs` documents.
//!   Static dispatch avoids the question: the call site names the exact method.

use std::collections::{BTreeSet, HashMap};

use crate::errors::CompileError;
use crate::parser::ast::{ClassMethod, Program, Stmt, StmtKind, TypeExpr, TypeParam};
use crate::span::Span;

use super::{instantiated_name, Bindings};

/// A method that declares type parameters of its own, lifted out of its class.
#[derive(Debug, Clone)]
pub struct MethodTemplate {
    /// The owning class as declared, which the instantiated method is attached to.
    pub declared_class: String,
    /// The method as declared, which the instantiated name is built from.
    pub declared_method: String,
    pub type_params: Vec<TypeParam>,
    /// Declared parameter NAMES in order. A named argument binds by name, so ordering a call
    /// against the declaration needs them — pairing by source position reads the bindings off the
    /// wrong arguments.
    pub param_names: Vec<String>,
    /// Declared parameter types in order; `None` for a parameter with no hint, which constrains
    /// nothing and is what makes `callable $f` unable to determine anything.
    pub params: Vec<Option<TypeExpr>>,
    /// Declared element type on the variadic parameter (`T ...$xs`), if any. It is a binding
    /// position like the fixed ones, and it lives in its own field rather than in `params`.
    pub variadic_type: Option<TypeExpr>,
    pub return_type: Option<TypeExpr>,
}

/// The key a template is found by: the class and the method, both normalized.
pub fn template_key(class: &str, method: &str) -> (String, String) {
    (
        super::classes::template_key(class),
        method.to_ascii_lowercase(),
    )
}

/// Every generic method the program declares.
///
/// Recollected each round rather than once, because a method template rides along when its class
/// is instantiated: `Box<int>` gets `map<U>` from `Box<T>`, and only then can a call on a
/// `Box<int>` find it.
pub fn collect(program: &Program) -> HashMap<(String, String), MethodTemplate> {
    let mut templates = HashMap::new();
    collect_into(program, &mut templates);
    templates
}

fn collect_into(stmts: &[Stmt], templates: &mut HashMap<(String, String), MethodTemplate>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::ClassDecl { name, methods, .. } => {
                for method in methods {
                    if method.type_params.is_empty() {
                        continue;
                    }
                    templates.insert(
                        template_key(name, &method.name),
                        MethodTemplate {
                            declared_class: name.clone(),
                            declared_method: method.name.clone(),
                            type_params: method.type_params.clone(),
                            param_names: method
                                .params
                                .iter()
                                .map(|(name, _, _, _)| name.clone())
                                .collect(),
                            params: method
                                .params
                                .iter()
                                .map(|(_, declared, _, _)| declared.clone())
                                .collect(),
                            variadic_type: method.variadic_type.clone(),
                            return_type: method.return_type.clone(),
                        },
                    );
                }
            }
            StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body) => {
                collect_into(body, templates)
            }
            _ => {}
        }
    }
}

/// What one round of checking asked for.
#[derive(Debug, Default, Clone)]
pub struct Requested {
    /// The instantiations to splice, as (class key, method key, type arguments).
    pub instantiations: Vec<((String, String), Bindings)>,
    /// (enclosing function, call site) -> the instantiated method name that site selected.
    ///
    /// A SET for the same reason the class map is one: a `Span` carries no file identity, so two
    /// same-named functions in two included files collide, and that residue is rejected rather
    /// than resolved.
    pub sites: HashMap<(String, Span), BTreeSet<String>>,
}

impl Requested {
    /// Returns whether this round asked for nothing, so the fixpoint can stop.
    pub fn is_empty(&self) -> bool {
        self.instantiations.is_empty() && self.sites.is_empty()
    }
}

/// Splices the requested method instantiations and renames each call site to the one it selected.
pub fn instantiate(
    program: Program,
    templates: &HashMap<(String, String), MethodTemplate>,
    requested: &Requested,
) -> Result<(Program, usize), CompileError> {
    if requested.is_empty() {
        return Ok((program, 0));
    }
    let mut names: HashMap<(String, Span), String> = HashMap::new();
    for (site, selected) in &requested.sites {
        if selected.len() > 1 {
            // Same position, same enclosing name, two different methods. Renaming it to either
            // one compiles a program that calls the wrong method at one of the two places it
            // appears, and nothing afterwards could notice — the AST would name a method that
            // exists. The same "no silent wrong output" rule the construction sites follow.
            return Err(CompileError::new(
                site.1,
                &format!(
                    "This call selects more than one instantiation of a generic method ({}); \
                     two included files declare a function named '{}' with a call at the same \
                     position, and a span cannot tell them apart",
                    selected.iter().cloned().collect::<Vec<_>>().join(", "),
                    site.0
                ),
            ));
        }
        if let Some(only) = selected.iter().next() {
            names.insert(site.clone(), only.clone());
        }
    }

    let mut added = 0usize;
    let mut program = program;
    for ((class_key, method_key), bindings) in &requested.instantiations {
        let Some(template) = templates.get(&(class_key.clone(), method_key.clone())) else {
            continue;
        };
        let instantiated = instantiated_name(&template.declared_method, bindings);
        if attach(&mut program, class_key, &template.declared_method, &instantiated, bindings) {
            added += 1;
        }
    }

    let mut pass = Rename {
        names,
        enclosing: Vec::new(),
    };
    Ok((
        crate::magic_constants::walker::walk_program(program, &mut pass),
        added,
    ))
}

/// Adds one instantiated method to its class, and reports whether it was new.
fn attach(
    program: &mut Program,
    class_key: &str,
    declared_method: &str,
    instantiated: &str,
    bindings: &Bindings,
) -> bool {
    fn visit(
        stmts: &mut [Stmt],
        class_key: &str,
        declared_method: &str,
        instantiated: &str,
        bindings: &Bindings,
    ) -> bool {
        for stmt in stmts.iter_mut() {
            match &mut stmt.kind {
                StmtKind::ClassDecl { name, methods, .. } => {
                    if super::classes::template_key(name) != class_key {
                        continue;
                    }
                    if methods
                        .iter()
                        .any(|method| method.name.eq_ignore_ascii_case(instantiated))
                    {
                        return false;
                    }
                    let Some(template) = methods
                        .iter()
                        .find(|method| method.name.eq_ignore_ascii_case(declared_method))
                        .cloned()
                    else {
                        return false;
                    };
                    methods.push(substitute(template, instantiated, bindings));
                    return true;
                }
                StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body) => {
                    if visit(body, class_key, declared_method, instantiated, bindings) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }
    visit(program, class_key, declared_method, instantiated, bindings)
}

/// Rewrites one template into a concrete method under its instantiated name.
///
/// The body is left as written: a type parameter can only reach it through a declared type, and
/// every one of those is substituted here. `type_params` is emptied, which is what stops the
/// checker from treating the result as a template again.
fn substitute(mut method: ClassMethod, instantiated: &str, bindings: &Bindings) -> ClassMethod {
    method.name = instantiated.to_string();
    method.type_params = Vec::new();
    for (_, declared, _, _) in method.params.iter_mut() {
        if let Some(ty) = declared.as_mut() {
            *ty = ty.substitute_type_params(bindings);
        }
    }
    if let Some(ty) = method.variadic_type.as_mut() {
        *ty = ty.substitute_type_params(bindings);
    }
    if let Some(ty) = method.return_type.as_mut() {
        *ty = ty.substitute_type_params(bindings);
    }
    method.body = substitute_body(std::mem::take(&mut method.body), bindings);
    method
}

/// Substitutes the type parameters inside a body, at every type position the walker reaches.
fn substitute_body(body: Vec<Stmt>, bindings: &Bindings) -> Vec<Stmt> {
    let mut pass = Substitute {
        bindings: bindings.clone(),
    };
    crate::magic_constants::walker::walk_program(body, &mut pass)
}

/// Renames the calls the checker resolved, keyed by enclosing function and position.
struct Rename {
    names: HashMap<(String, Span), String>,
    /// The functions the walk is lexically inside, innermost last.
    ///
    /// Half the key, for the reason a generic function call documents: one position inside a
    /// template is reached once per instantiation of it and legitimately selects a different
    /// method each time. Top-level statements lower inside the synthetic `main` and the checker
    /// has no enclosing function for them, so both sides spell that one name the same way or the
    /// lookup misses and the call is never renamed.
    enclosing: Vec<String>,
}

impl crate::magic_constants::walker::Pass for Rename {
    /// Required by the trait; this pass rewrites no magic constant.
    fn transform_magic(
        &self,
        _span: Span,
        mc: crate::parser::ast::MagicConstant,
    ) -> crate::parser::ast::ExprKind {
        crate::parser::ast::ExprKind::MagicConstant(mc)
    }

    fn transform_method_call_name(&self, method: String, span: Span) -> String {
        let scope = self
            .enclosing
            .last()
            .cloned()
            .unwrap_or_else(|| "main".to_string());
        match self.names.get(&(scope, span)) {
            Some(instantiated) => instantiated.clone(),
            None => method,
        }
    }

    fn enter_function(&mut self, name: &str) {
        self.enclosing.push(name.to_string());
    }

    fn leave_function(&mut self) {
        self.enclosing.pop();
    }
}

/// Substitutes bound type parameters at every type position of an instantiated body.
struct Substitute {
    bindings: Bindings,
}

impl crate::magic_constants::walker::Pass for Substitute {
    /// Required by the trait; this pass rewrites no magic constant.
    fn transform_magic(
        &self,
        _span: Span,
        mc: crate::parser::ast::MagicConstant,
    ) -> crate::parser::ast::ExprKind {
        crate::parser::ast::ExprKind::MagicConstant(mc)
    }

    fn transform_type(&self, ty: TypeExpr, _span: Span) -> TypeExpr {
        ty.substitute_type_params(&self.bindings)
    }
}

/// Removes every generic method template, once every call has its own instantiation.
///
/// The parallel to `generics::strip_templates` is exact: a template's annotations name types
/// with no representation, so it must never reach lowering. The difference is only that a method
/// template lives on a class that DOES survive, so the class is kept and the method dropped.
pub fn strip_templates(program: Program) -> Program {
    fn visit(stmts: &mut Vec<Stmt>) {
        for stmt in stmts.iter_mut() {
            match &mut stmt.kind {
                StmtKind::ClassDecl { methods, .. } => {
                    methods.retain(|method| method.type_params.is_empty());
                }
                StmtKind::InterfaceDecl { methods, .. } => {
                    methods.retain(|method| method.type_params.is_empty());
                }
                StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body) => visit(body),
                _ => {}
            }
        }
    }
    let mut program = program;
    visit(&mut program);
    program
}
