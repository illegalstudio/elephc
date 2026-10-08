//! Purpose:
//! Tracks, during an AST walk, the scope half of the keys the checker records generic sites under.
//!
//! Called from:
//! - `crate::generics::classes::Instantiate` (inferred constructions and static factories).
//! - `crate::generics::methods::Rename` (inferred generic method calls).
//!
//! Key details:
//! - The checker keys `generic_call_sites`, `generic_new_sites` and `generic_method_sites` by
//!   `current_loop_storage_scope`, and EIR lowering reads `generic_call_sites` by its own
//!   `loop_storage_scope`. Both spell a body's scope the same way: `main` for top-level code, the
//!   declared name for a function, `Class::method` for a method, and
//!   `nested_loop_storage_scope(parent, span)` for a closure or arrow function. This tracker builds
//!   that same string from the walker's enter/leave hooks, so the AST passes that rename a site
//!   look it up under the key the checker wrote.
//! - The scope is half the key because one source position inside a template is reached once per
//!   instantiation and legitimately resolves differently each time, so the position alone is not
//!   enough. Spelling it differently on one side makes the lookup miss.
//! - A trait body is the exception the tracker cannot spell: the checker flattens the trait into
//!   each using class and records `Class::method`, while this walk visits the trait declaration
//!   once. `resolve` matches such a site by its span and the scope below the trait instead, and
//!   answers only when every using class agrees, since one AST node can carry one name.

use std::collections::HashMap;

use crate::span::Span;

/// The scope the walk is lexically inside, built the way the checker builds its scope.
#[derive(Default)]
pub(crate) struct SiteScope {
    /// The class-like declarations the walk is inside, innermost last.
    classes: Vec<String>,
    /// The body scopes the walk is inside, innermost last; empty means top-level `main`.
    scopes: Vec<String>,
    /// The trait declaration the walk is inside, if any. Traits do not nest.
    current_trait: Option<String>,
}

impl SiteScope {
    /// Returns the scope a site at the current position is keyed under.
    pub(crate) fn current(&self) -> String {
        self.scopes
            .last()
            .cloned()
            .unwrap_or_else(|| "main".to_string())
    }

    /// Enters a class, interface, enum or trait declaration.
    pub(crate) fn enter_class(&mut self, name: &str) {
        self.classes.push(name.to_string());
    }

    /// Leaves the innermost class-like declaration.
    pub(crate) fn leave_class(&mut self) {
        self.classes.pop();
    }

    /// Enters a trait declaration, whose method bodies the checker sees under each using class.
    pub(crate) fn enter_trait(&mut self, name: &str) {
        self.classes.push(name.to_string());
        self.current_trait = Some(name.to_string());
    }

    /// Leaves the trait declaration.
    pub(crate) fn leave_trait(&mut self) {
        self.classes.pop();
        self.current_trait = None;
    }

    /// Returns the name the checker recorded for the site at `span` in the current scope.
    ///
    /// Inside a trait, the site was recorded once per using class (`C::make`, `D::make`) and never
    /// under the trait's own name, so it is matched by span and by the scope below the trait. Two
    /// using classes that resolved it differently leave it unanswered: the trait has one AST node
    /// for the site, and picking either name would give the other class the wrong one.
    pub(crate) fn resolve<'m>(
        &self,
        names: &'m HashMap<(String, Span), String>,
        span: Span,
    ) -> Option<&'m String> {
        let scope = self.current();
        if let Some(found) = names.get(&(scope.clone(), span)) {
            return Some(found);
        }
        let below_trait = scope.strip_prefix(&format!("{}::", self.current_trait.as_ref()?))?;
        let suffix = format!("::{}", below_trait);
        let mut answers = names
            .iter()
            .filter(|((key_scope, key_span), _)| *key_span == span && key_scope.ends_with(&suffix))
            .map(|(_, name)| name);
        let first = answers.next()?;
        answers.all(|name| name == first).then_some(first)
    }

    /// Enters a top-level function body, keyed by its declared name.
    pub(crate) fn enter_function(&mut self, name: &str) {
        self.scopes.push(name.to_string());
    }

    /// Enters a method body, keyed `Class::method` as the checker's method pass and lowering do.
    pub(crate) fn enter_method(&mut self, name: &str) {
        let scope = match self.classes.last() {
            Some(class) => format!("{}::{}", class, name),
            None => name.to_string(),
        };
        self.scopes.push(scope);
    }

    /// Enters a closure or arrow function body declared at `span`.
    pub(crate) fn enter_closure(&mut self, span: Span) {
        let scope = crate::types::nested_loop_storage_scope(&self.current(), span);
        self.scopes.push(scope);
    }

    /// Leaves the innermost function, method or closure body.
    pub(crate) fn leave_body(&mut self) {
        self.scopes.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the tracker spells each body's scope the way the checker and lowering do.
    #[test]
    fn scope_matches_the_checker_spelling_for_every_body_kind() {
        let mut scope = SiteScope::default();
        assert_eq!(scope.current(), "main");
        scope.enter_function("helper");
        assert_eq!(scope.current(), "helper");
        scope.leave_body();
        scope.enter_class("Box<int>");
        scope.enter_method("get");
        assert_eq!(scope.current(), "Box<int>::get");
        let span = Span::new(3, 9);
        scope.enter_closure(span);
        assert_eq!(
            scope.current(),
            crate::types::nested_loop_storage_scope("Box<int>::get", span)
        );
        scope.leave_body();
        scope.leave_body();
        scope.leave_class();
        assert_eq!(scope.current(), "main");
    }

    /// Verifies a site in a trait body resolves through the using classes' keys, and only when
    /// they agree.
    #[test]
    fn trait_body_sites_resolve_through_the_using_classes() {
        let span = Span::new(4, 20);
        let mut names = HashMap::new();
        names.insert(("C::make".to_string(), span), "Box<int>".to_string());
        names.insert(("D::make".to_string(), span), "Box<int>".to_string());
        let mut scope = SiteScope::default();
        scope.enter_trait("Tr");
        scope.enter_method("make");
        assert_eq!(scope.resolve(&names, span).map(String::as_str), Some("Box<int>"));
        names.insert(("E::make".to_string(), span), "Box<string>".to_string());
        assert_eq!(scope.resolve(&names, span), None);
        scope.leave_body();
        scope.leave_trait();
        assert_eq!(scope.resolve(&names, span), None);
    }
}
