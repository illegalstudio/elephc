//! Purpose:
//! Proves which declaration names are unreferenced for web environment callable demand.
//!
//! Called from:
//! - `super::usage::has_reachable_unknown_callable`, before web prelude injection.
//!
//! Key details:
//! - Executable references and escaped literals grow a conservative declaration closure.
//! - This only excludes opaque callable facts, never direct environment or eval observers.
//! - Reflection, introspection and dynamic construction disable the exclusion proof.

use std::collections::{HashMap, HashSet};

use crate::names::php_symbol_key;
use crate::parser::ast::StaticReceiver;
use super::usage::Usage;

/// Source references that can enter function or method bodies before namespace resolution.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct DeclarationRoots {
    symbols: HashSet<String>,
    methods: HashSet<(String, String)>,
    inherited_classes: HashSet<String>,
    interfaces: HashSet<String>,
    opaque_interfaces: HashSet<String>,
}

/// Native constructor roles collected independently of whether declarations can be excluded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ConstructorMetadata {
    constructor_callbacks: HashMap<String, ConstructorCallbacks>,
    class_edges: HashMap<String, HashSet<String>>,
}

/// Native callback slots, or a source constructor whose signature cannot inherit those slots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ConstructorCallbacks {
    Slots(HashSet<usize>),
    Opaque,
}

impl ConstructorCallbacks {
    /// Unions ambiguous pre-resolution targets, with an opaque signature taking precedence.
    fn extend(&mut self, other: &Self) {
        if matches!(other, Self::Opaque) {
            *self = Self::Opaque;
        } else if let (Self::Slots(current), Self::Slots(next)) = (self, other) {
            current.extend(next);
        }
    }
}

impl DeclarationRoots {
    /// Seeds implicit interface entries without treating calls inside unused helpers as roots.
    pub(super) fn implicit_only(mut self, usage: &Usage) -> Self {
        self.symbols.clear();
        self.methods = interface_entries(usage);
        self
    }

    /// Grows the executable closure monotonically, reporting whether another scan is needed.
    pub(super) fn expand(&mut self, next: Self) -> bool {
        let before = self.symbols.len() + self.methods.len();
        self.symbols.extend(next.symbols);
        self.methods.extend(next.methods);
        before != self.symbols.len() + self.methods.len()
    }

    /// Unknown external interfaces keep every implementation method as a possible implicit entry.
    pub(super) fn knows_interface(&self, name: &str) -> bool {
        let key = php_symbol_key(name);
        let leaf = symbol_leaf(&key);
        self.interfaces.contains(leaf) && !self.opaque_interfaces.contains(leaf)
    }
}

impl ConstructorMetadata {
    /// Collects roles after the complete declaration scan, including forward aliases and parents.
    pub(super) fn collect(usage: &Usage) -> Self {
        Self {
            constructor_callbacks: callback_constructors(usage),
            class_edges: usage.callable_class_edges.clone(),
        }
    }

    /// Resolves constructor callback roles without certifying an overridden native signature.
    pub(super) fn constructor_callbacks_for_receiver(
        &self, receiver: &StaticReceiver, current_class: Option<&str>,
    ) -> Result<Option<ConstructorCallbacks>, ()> {
        let current = || current_class.map(php_symbol_key).ok_or(());
        match receiver.written_class_receiver() {
            StaticReceiver::Named(name) => Ok(self.constructor_callbacks
                .get(symbol_leaf(&php_symbol_key(&name.as_canonical()))).cloned()),
            StaticReceiver::Self_ | StaticReceiver::Static => Ok(self.constructor_callbacks
                .get(symbol_leaf(&current()?)).cloned()),
            StaticReceiver::Parent => {
                let class = current()?;
                let parents = self.class_edges.get(symbol_leaf(&class)).ok_or(())?;
                let mut callbacks: Option<ConstructorCallbacks> = None;
                for parent in parents {
                    if let Some(next) = self.constructor_callbacks.get(parent) {
                        if let Some(current) = &mut callbacks { current.extend(next); }
                        else { callbacks = Some(next.clone()); }
                    }
                }
                Ok(callbacks)
            }
            _ => Err(()),
        }
    }
}

/// Returns conservative symbol roots, or no proof when runtime lookup can discover other names.
pub(super) fn declaration_roots(usage: &Usage) -> Option<DeclarationRoots> {
    if usage.introspects || usage.constructs_dynamic_class
        || usage.classes.iter().any(|name| symbol_leaf(name).starts_with("reflection"))
        || usage.methods.iter().any(|name| matches!(name.as_str(), "invoke" | "invokeargs"))
    {
        return None;
    }
    let symbols = usage.functions.iter().chain(&usage.callable_wildcard_methods).chain(&usage.literals)
        .map(|name| symbol_leaf(name).to_owned()).collect();
    let mut methods = HashSet::new();
    for (class, method) in &usage.callable_static_methods {
        let mut pending = vec![symbol_leaf(class).to_owned()];
        let mut seen = HashSet::new();
        while let Some(class) = pending.pop() {
            if !seen.insert(class.clone()) { continue; }
            methods.insert((class.clone(), symbol_leaf(method).to_owned()));
            if let Some(targets) = usage.callable_class_edges.get(&class) {
                pending.extend(targets.iter().cloned());
            }
        }
    }
    methods.extend(interface_entries(usage));
    let mut inherited_classes = HashSet::new();
    let mut pending: Vec<_> = usage.callable_inherited_classes.iter()
        .map(|class| symbol_leaf(class).to_owned()).collect();
    while let Some(class) = pending.pop() {
        if !inherited_classes.insert(class.clone()) { continue; }
        if let Some(targets) = usage.callable_class_edges.get(&class) {
            pending.extend(targets.iter().cloned());
        }
    }
    let interfaces = usage.callable_interfaces.iter().map(|name| symbol_leaf(name).to_owned()).collect();
    let mut opaque_interfaces: HashSet<_> = elephc_builtin_contract::classes().iter()
        .filter(|class| class.kind == elephc_builtin_contract::ClassKind::Interface)
        .map(|class| symbol_leaf(&php_symbol_key(class.name)).to_owned()).collect();
    loop {
        let before = opaque_interfaces.len();
        for (alias, targets) in &usage.callable_class_edges {
            if targets.iter().any(|target| opaque_interfaces.contains(target)) {
                opaque_interfaces.insert(alias.clone());
            }
        }
        if before == opaque_interfaces.len() { break; }
    }
    Some(DeclarationRoots { symbols, methods, inherited_classes, interfaces, opaque_interfaces })
}

/// Tracks native constructors whose PHP callbacks execute outside the source AST walk.
fn callback_constructors(usage: &Usage) -> HashMap<String, ConstructorCallbacks> {
    let declared: HashSet<_> = usage.callable_constructor_classes.iter()
        .map(|name| symbol_leaf(name).to_owned()).collect();
    let mut constructors: HashMap<String, ConstructorCallbacks> = [
        ("fiber", 0), ("callbackfilteriterator", 1), ("recursivecallbackfilteriterator", 1),
    ].into_iter().map(|(name, index)| (name.to_owned(), ConstructorCallbacks::Slots(HashSet::from([index])))).collect();
    loop {
        let before = constructors.clone();
        for (alias, targets) in &usage.callable_class_edges {
            for callbacks in targets.iter().filter_map(|target| before.get(target)) {
                let next = if declared.contains(alias) { ConstructorCallbacks::Opaque }
                    else { callbacks.clone() };
                constructors.entry(alias.clone()).and_modify(|current| current.extend(&next))
                    .or_insert(next);
            }
        }
        if before == constructors { break; }
    }
    constructors
}

/// Roots source interface obligations on their implementing classes, including aliases and parents.
fn interface_entries(usage: &Usage) -> HashSet<(String, String)> {
    let mut contracts: HashMap<String, HashSet<String>> = HashMap::new();
    for (name, methods) in &usage.callable_interface_methods {
        contracts.entry(symbol_leaf(name).to_owned()).or_default().extend(methods.iter().cloned());
    }
    let mut entries = HashSet::new();
    for (class, interfaces) in &usage.callable_implementations {
        let mut pending: Vec<_> = interfaces.iter().map(|name| symbol_leaf(name).to_owned()).collect();
        let mut seen = HashSet::new();
        while let Some(interface) = pending.pop() {
            if !seen.insert(interface.clone()) { continue; }
            if let Some(methods) = contracts.get(&interface) {
                entries.extend(methods.iter().map(|method| (symbol_leaf(class).to_owned(), method.clone())));
            }
            if let Some(parents) = usage.callable_class_edges.get(&interface) {
                pending.extend(parents.iter().cloned());
            }
        }
    }
    entries
}

/// Keeps compiler-owned callbacks and magic methods even when no PHP call spells their names.
pub(super) fn is_referenced(roots: &DeclarationRoots, name: &str) -> bool {
    let key = php_symbol_key(name);
    let leaf = symbol_leaf(&key);
    leaf.starts_with("__") || roots.symbols.contains(leaf)
        || key.split_once("::").is_some_and(|(class, method)| {
            roots.inherited_classes.contains(symbol_leaf(class))
                || roots.methods.contains(&(symbol_leaf(class).to_owned(), method.to_owned()))
        })
}

/// Adds a conservative alias or inheritance edge, unioned across namespace scopes.
pub(super) fn class_edge(edges: &mut HashMap<String, HashSet<String>>, from: &str, to: &str) {
    edges.entry(symbol_leaf(&php_symbol_key(from)).to_owned()).or_default()
        .insert(symbol_leaf(&php_symbol_key(to)).to_owned());
}

/// Compares leaf spellings before namespace resolution, including escaped `Class::method` names.
fn symbol_leaf(name: &str) -> &str {
    let member = name.rsplit("::").next().unwrap_or(name);
    let base = member.split('<').next().unwrap_or(member);
    base.rsplit('\\').next().unwrap_or(base)
}
