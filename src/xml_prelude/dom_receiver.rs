//! Purpose:
//! Tracks receiver provenance for DOMDocument XML-loading capability detection.
//!
//! Called from:
//! - The XML prelude detector while visiting expressions and declaration scopes.
//!
//! Key details:
//! - Known unrelated receivers never opt into libxml2 just by naming loadXML.
//! - Boxed receivers remain conservative only in programs that reference the DOM surface.
//! - Assignment facts accumulate across branches so either possible DOM receiver counts.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind, TypeExpr};

type Classes = Option<HashSet<String>>;
type Parameter = (String, Option<TypeExpr>, Option<Expr>, bool);

/// Shared declaration facts resolved with the detector's global fallback symbols.
#[derive(Default)]
struct Declarations {
    classes: HashSet<String>,
    parents: HashMap<String, String>,
    properties: HashMap<(String, String), Classes>,
    returns: HashMap<String, Classes>,
}

/// A lexical scope's possible receiver classes, separate from compiler type inference.
pub(super) struct DomReceiver {
    declarations: Rc<Declarations>,
    variables: RefCell<HashMap<String, Classes>>,
    class: Option<String>,
    dom_available: bool,
}

/// Canonicalizes seeded DOMDocument separately from user-owned classes with that name.
fn class_key(name: &Name) -> String {
    if crate::name_resolver::is_additional_global_symbol(name, "DOMDocument") {
        "#domdocument".to_string()
    } else {
        php_symbol_key(name.as_str())
    }
}

/// Extracts possible declared classes without treating mixed/scalar types as known objects.
fn type_classes(ty: &TypeExpr) -> Classes {
    match ty {
        TypeExpr::Named(name) | TypeExpr::GenericClass { name, .. } => {
            if ["mixed", "object", "array", "callable", "null"].iter().any(|ty| name.as_str().eq_ignore_ascii_case(ty)) {
                return None;
            }
            Some(HashSet::from([class_key(name)]))
        }
        TypeExpr::Nullable(inner) => type_classes(inner),
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            let mut classes = HashSet::new();
            for member in members {
                classes.extend(type_classes(member)?);
            }
            Some(classes)
        }
        _ => None,
    }
}

/// Collects class inheritance, property hints, and function return hints before walking calls.
fn collect_declarations(program: &[Stmt], declarations: &mut Declarations) {
    for stmt in program {
        match &stmt.kind {
            StmtKind::ClassDecl { name, extends, properties, methods, .. } => {
                let class = php_symbol_key(name);
                declarations.classes.insert(class.clone());
                if let Some(parent) = extends {
                    declarations.parents.insert(class.clone(), class_key(parent));
                }
                for property in properties {
                    let ty = property.type_expr.as_ref().and_then(type_classes);
                    declarations.properties.insert((class.clone(), property.name.clone()), ty);
                }
                for method in methods {
                    collect_declarations(&method.body, declarations);
                }
            }
            StmtKind::FunctionDecl { name, return_type, body, .. } => {
                declarations.returns.insert(php_symbol_key(name), return_type.as_ref().and_then(type_classes));
                collect_declarations(body, declarations);
            }
            StmtKind::Synthetic(body) | StmtKind::NamespaceBlock { body, .. }
            | StmtKind::IncludeOnceGuard { body, .. } => collect_declarations(body, declarations),
            _ => {}
        }
    }
}

impl DomReceiver {
    /// Creates the top-level scope with all known declaration hints.
    pub(super) fn new(program: &[Stmt], dom_available: bool) -> Self {
        let mut declarations = Declarations::default();
        collect_declarations(program, &mut declarations);
        Self {
            declarations: Rc::new(declarations), variables: RefCell::new(HashMap::new()),
            class: None, dom_available,
        }
    }

    /// Starts a function or method scope; closures retain outer capture provenance.
    pub(super) fn nested(&self, params: &[Parameter], capture: bool) -> Self {
        let result = Self {
            declarations: self.declarations.clone(),
            variables: RefCell::new(if capture { self.variables.borrow().clone() } else { HashMap::new() }),
            class: self.class.clone(), dom_available: self.dom_available,
        };
        for (name, ty, _, _) in params {
            result.variables.borrow_mut().insert(name.clone(), ty.as_ref().and_then(type_classes));
        }
        result
    }

    /// Supplies the class for `$this` in a member body.
    pub(super) fn in_class(&self, name: &str) -> Self {
        let mut result = self.nested(&[], false);
        result.class = Some(php_symbol_key(name));
        result
    }

    /// Accumulates possible assignment types instead of overwriting a different branch's facts.
    fn assign(&self, name: &str, classes: Classes) {
        let mut variables = self.variables.borrow_mut();
        match variables.get_mut(name) {
            Some(Some(old)) => match classes {
                Some(classes) => old.extend(classes),
                None => { variables.insert(name.to_string(), None); }
            },
            Some(None) => {}
            None => { variables.insert(name.to_string(), classes); }
        }
    }

    /// Records local assignments before later expressions inspect their receiver.
    pub(super) fn observe_statement(&self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Assign { name, value } => self.assign(name, self.classes(value)),
            StmtKind::TypedAssign { name, type_expr, .. } => self.assign(name, type_classes(type_expr)),
            StmtKind::RefAssign { target, source } => {
                self.assign(target, None);
                if let ExprKind::Variable(name) = &source.kind { self.assign(name, None); }
            }
            StmtKind::Foreach { key_var, value_var, .. } => {
                self.assign(value_var, None);
                if let Some(name) = key_var { self.assign(name, None); }
            }
            _ => {}
        }
    }

    /// Records assignment expressions, including ones nested in call arguments.
    pub(super) fn observe_expression(&self, expr: &Expr) {
        if let ExprKind::Closure { capture_refs, .. } = &expr.kind {
            for name in capture_refs { self.assign(name, None); }
        }
        if let ExprKind::Assignment { target, value, .. } = &expr.kind {
            if let ExprKind::Variable(name) = &target.kind {
                self.assign(name, self.classes(value));
            }
        }
        let args = match &expr.kind {
            ExprKind::FunctionCall { args, .. } | ExprKind::MethodCall { args, .. }
            | ExprKind::NullsafeMethodCall { args, .. } | ExprKind::NullsafeDynamicMethodCall { args, .. }
            | ExprKind::StaticMethodCall { args, .. } | ExprKind::ExprCall { args, .. }
            | ExprKind::ClosureCall { args, .. } | ExprKind::NewObject { args, .. }
            | ExprKind::NewGeneric { args, .. } | ExprKind::NewDynamic { args, .. }
            | ExprKind::NewDynamicObject { args, .. } | ExprKind::NewScopedObject { args, .. } => args,
            _ => return,
        };
        for arg in args {
            let arg = if let ExprKind::NamedArg { value, .. } = &arg.kind { value } else { arg };
            if let ExprKind::Variable(name) = &arg.kind { self.assign(name, None); }
        }
    }

    /// Infers only provenance supported by explicit constructors, annotations, and aliases.
    fn classes(&self, expr: &Expr) -> Classes {
        match &expr.kind {
            ExprKind::NewObject { class_name, .. } => Some(HashSet::from([class_key(class_name)])),
            ExprKind::NewGeneric { class_type, .. } => type_classes(class_type),
            ExprKind::Variable(name) => self.variables.borrow().get(name).cloned().flatten(),
            ExprKind::This => self.class.clone().map(|class| HashSet::from([class])),
            ExprKind::FunctionCall { name, .. } => self.declarations.returns.get(&php_symbol_key(name.as_str())).cloned().flatten(),
            ExprKind::Clone(value) | ExprKind::ErrorSuppress(value) => self.classes(value),
            ExprKind::Assignment { value, .. } => self.classes(value),
            ExprKind::PropertyAccess { object, property } | ExprKind::NullsafePropertyAccess { object, property } => {
                let mut result = HashSet::new();
                for mut class in self.classes(object)? {
                    let mut visited = HashSet::new();
                    loop {
                        if !visited.insert(class.clone()) { return None; }
                        if let Some(classes) = self.declarations.properties.get(&(class.clone(), property.clone())) {
                            result.extend(classes.clone()?);
                            break;
                        }
                        class = self.declarations.parents.get(&class)?.clone();
                    }
                }
                Some(result)
            }
            ExprKind::Ternary { then_expr, else_expr, .. } => {
                let mut result = self.classes(then_expr)?;
                result.extend(self.classes(else_expr)?);
                Some(result)
            }
            _ => None,
        }
    }

    /// Recognizes DOMDocument and its subclasses, excluding known unrelated receivers.
    pub(super) fn may_load_xml(&self, object: &Expr) -> bool {
        let Some(classes) = self.classes(object) else { return self.dom_available; };
        classes.into_iter().any(|mut class| {
            let mut visited = HashSet::new();
            loop {
                if class == "#domdocument" { return true; }
                if !visited.insert(class.clone()) { return false; }
                let Some(parent) = self.declarations.parents.get(&class) else {
                    let builtin = crate::types::builtin_classes::builtin_class_like_names()
                        .any(|name| php_symbol_key(name) == class);
                    return !builtin && !self.declarations.classes.contains(&class) && self.dom_available;
                };
                class = parent.clone();
            }
        })
    }
}
