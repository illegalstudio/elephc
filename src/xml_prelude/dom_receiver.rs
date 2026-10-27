//! Purpose:
//! Tracks receiver provenance for DOMDocument XML-loading capability detection.
//!
//! Called from:
//! - The XML prelude detector while visiting expressions and declaration scopes.
//!
//! Key details:
//! - Known unrelated receivers never opt into libxml2 just by naming loadXML.
//! - Boxed receivers remain conservative only in programs that reference the DOM surface.
//! - Assignments replace facts; explicit branch and loop joins preserve alternative receivers.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind, TypeExpr};

mod aliases;
mod declarations;
mod signatures;
use declarations::{collect_declarations, Declarations};

type Classes = Option<HashSet<String>>;
type Parameter = (String, Option<TypeExpr>, Option<Expr>, bool);

/// A lexical scope's possible receiver classes, separate from compiler type inference.
#[derive(Clone)]
pub(super) struct DomReceiver {
    declarations: Rc<Declarations>,
    variables: RefCell<HashMap<String, Classes>>,
    callables: RefCell<HashMap<String, crate::types::FunctionSig>>,
    escaped_refs: RefCell<HashSet<String>>,
    aliases: RefCell<HashMap<String, HashSet<String>>>,
    uncertain_aliases: RefCell<HashSet<String>>,
    global_scope: bool,
    observers: Vec<Rc<Self>>,
    class: Option<String>,
    dom_available: bool,
}

/// Separates seeded DOM ancestors from user-owned classes with the same names.
fn class_key(name: &Name) -> String {
    if crate::name_resolver::is_additional_global_symbol(name, "DOMDocument") {
        "#domdocument".to_string()
    } else if crate::name_resolver::is_additional_global_symbol(name, "DOMNode") {
        "#domnode".to_string()
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

impl DomReceiver {
    /// Creates the top-level scope with all known declaration hints.
    pub(super) fn new(program: &[Stmt], dom_available: bool) -> Self {
        let mut declarations = Declarations::default();
        if dom_available { collect_declarations(program, &mut declarations); }
        Self {
            declarations: Rc::new(declarations), variables: RefCell::new(HashMap::new()),
            callables: RefCell::new(HashMap::new()), escaped_refs: RefCell::new(HashSet::new()),
            aliases: RefCell::new(HashMap::new()), uncertain_aliases: RefCell::new(HashSet::new()),
            global_scope: true, observers: Vec::new(),
            class: None, dom_available,
        }
    }

    /// Starts a function or method scope; closures retain outer capture provenance.
    pub(super) fn nested(&self, params: &[Parameter], capture: bool) -> Self {
        let result = Self {
            declarations: self.declarations.clone(),
            variables: RefCell::new(if capture { self.variables.borrow().clone() } else { HashMap::new() }),
            callables: RefCell::new(if capture { self.callables.borrow().clone() } else { HashMap::new() }),
            escaped_refs: RefCell::new(if capture { self.escaped_refs.borrow().clone() } else { HashSet::new() }),
            aliases: RefCell::new(if capture { self.aliases.borrow().clone() } else { HashMap::new() }),
            uncertain_aliases: RefCell::new(if capture { self.uncertain_aliases.borrow().clone() } else { HashSet::new() }),
            global_scope: false, observers: Vec::new(),
            class: self.class.clone(), dom_available: self.dom_available,
        };
        for (name, ty, _, _) in params.iter().filter(|_| self.dom_available) {
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

    /// Records an unconditional assignment; control-flow joins are handled separately.
    fn assign(&self, name: &str, classes: Classes) {
        if !self.dom_available { return; }
        self.write_reference_fact(name, classes);
        self.notify_observers();
    }

    /// Collects every possible throw-prefix state independently of successful try completion.
    pub(super) fn exception_observer(&self) -> Rc<Self> {
        let mut result = self.clone();
        result.observers.clear();
        Rc::new(result)
    }

    /// Records mutations into a try region's exception-prefix accumulator.
    pub(super) fn watch_exceptions(&mut self, observer: Rc<Self>) { self.observers.push(observer); }

    /// Updates active exception accumulators after releasing all mutable fact borrows.
    fn notify_observers(&self) {
        for observer in &self.observers { observer.join(self); }
    }

    /// Replaces this path's facts after an unconditional control-flow region.
    pub(super) fn replace(&self, other: &Self) {
        *self.variables.borrow_mut() = other.variables.borrow().clone();
        *self.callables.borrow_mut() = other.callables.borrow().clone();
        *self.escaped_refs.borrow_mut() = other.escaped_refs.borrow().clone();
        *self.aliases.borrow_mut() = other.aliases.borrow().clone();
        *self.uncertain_aliases.borrow_mut() = other.uncertain_aliases.borrow().clone();
        self.notify_observers();
    }

    /// Joins alternative paths and reports whether any receiver fact changed.
    pub(super) fn join(&self, other: &Self) -> bool {
        let aliases_changed = self.join_references(other);
        let before = self.variables.borrow().clone();
        let right = other.variables.borrow();
        let mut joined = before.clone();
        for name in before.keys().chain(right.keys()) {
            let classes = match (before.get(name), right.get(name)) {
                (Some(Some(left)), Some(Some(right))) => Some(left.union(right).cloned().collect()),
                _ => None,
            };
            joined.insert(name.clone(), classes);
        }
        let mut callables = self.callables.borrow_mut();
        let callables_before = callables.clone();
        callables.retain(|name, sig| other.callables.borrow().get(name) == Some(sig));
        let refs_before = self.escaped_refs.borrow().len();
        self.escaped_refs.borrow_mut().extend(other.escaped_refs.borrow().iter().cloned());
        let changed = aliases_changed || joined != before || *callables != callables_before || refs_before != self.escaped_refs.borrow().len();
        *self.variables.borrow_mut() = joined;
        drop(callables);
        self.notify_observers();
        changed
    }

    /// Records local assignments before later expressions inspect their receiver.
    pub(super) fn observe_statement(&self, stmt: &Stmt) {
        if !self.dom_available { return; }
        match &stmt.kind {
            StmtKind::Global { vars } => {
                for name in vars {
                    self.assign(name, None);
                    self.escaped_refs.borrow_mut().insert(name.clone());
                }
            }
            StmtKind::Assign { name, value } => {
                let sig = self.callable_signature(value);
                self.assign(name, self.classes(value));
                self.set_callable(name, sig);
            },
            StmtKind::TypedAssign { name, type_expr, .. } => self.assign(name, type_classes(type_expr)),
            StmtKind::RefAssign { target, source } => self.bind_reference(target, source),
            StmtKind::Foreach { key_var, value_var, .. } => {
                self.assign(value_var, None);
                if let Some(name) = key_var { self.assign(name, None); }
            }
            _ => {}
        }
    }

    /// Records assignment expressions, including ones nested in call arguments.
    pub(super) fn observe_expression(&self, expr: &Expr) {
        if !self.dom_available { return; }
        if let ExprKind::Assignment { target, value, .. } = &expr.kind {
            if let ExprKind::Variable(name) = &target.kind {
                let signature = self.callable_signature(value);
                self.assign(name, self.classes(value));
                self.set_callable(name, signature);
            }
        }
    }

    /// Marks captured aliases before analyzing a closure and remembers later indirect mutations.
    pub(super) fn observe_captures(&self, expr: &Expr) {
        if !self.dom_available { return; }
        if let ExprKind::Closure { capture_refs, .. } = &expr.kind {
            for name in capture_refs {
                self.assign(name, None);
                self.escaped_refs.borrow_mut().insert(name.clone());
            }
        }
    }

    /// Applies call mutations after the receiver and source arguments were evaluated.
    pub(super) fn apply_mutations(&self, names: &[String]) {
        for name in names { self.assign(name, None); }
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

    /// Recognizes possible DOMDocument receivers, excluding only proven unrelated classes.
    pub(super) fn may_load_xml(&self, object: &Expr) -> bool {
        if !self.dom_available { return false; }
        let Some(classes) = self.classes(object) else { return self.dom_available; };
        classes.into_iter().any(|mut class| {
            let mut visited = HashSet::new();
            loop {
                if matches!(class.as_str(), "#domdocument" | "#domnode")
                    || self.declarations.open_receivers.contains(&class) { return true; }
                if !visited.insert(class.clone()) { return false; }
                let Some(parent) = self.declarations.parents.get(&class) else {
                    if let Some(builtin) = elephc_builtin_contract::lookup_class(&class) {
                        return builtin.kind == elephc_builtin_contract::ClassKind::Interface;
                    }
                    return !self.declarations.classes.contains(&class) && self.dom_available;
                };
                class = parent.clone();
            }
        })
    }
}
