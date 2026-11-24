//! Purpose:
//! Keeps the written positions of generic headers and scoped type references.
//!
//! Called from:
//! - The tagless runtime parser and its declaration and type productions.
//!
//! Key details:
//! - Token spans supplied by the runtime adapter identify the original token.
//! - Type references are associated with their declaration, including shadowed names.

use std::cell::RefCell;

use crate::syntax::{names::Name, span::Span};

/// Source positions retained while parsing templates for a runtime consumer.
#[derive(Default)]
pub struct RuntimeTemplateSyntax {
    pub headers: Vec<RuntimeTemplateHeader>,
    pub references: Vec<RuntimeTemplateReference>,
    pub extensions: Vec<Span>,
}

/// Token spans identifying one declaration's generic header.
pub struct RuntimeTemplateHeader {
    pub name: Span,
    pub open: Span,
    pub after: Span,
    pub parameters: Vec<Span>,
}

/// The use of a type parameter, distinguished from a same-named value or member.
pub struct RuntimeTemplateReference {
    pub usage: Span,
    pub parameter: Span,
}

#[derive(Default)]
struct Tracking {
    parameters: Vec<(String, Span)>,
    syntax: RuntimeTemplateSyntax,
}

thread_local! {
    static TRACKING: RefCell<Option<Tracking>> = const { RefCell::new(None) };
}

/// Owns one isolated set of recorded positions.
pub(super) struct Trace(Option<Tracking>);

impl Trace {
    /// Begins collecting runtime-only syntax metadata.
    pub(super) fn start() -> Self {
        Self(TRACKING.with(|tracking| tracking.replace(Some(Tracking::default()))))
    }

    /// Drains retained positions after parsing the fragment.
    pub(super) fn take(&self) -> RuntimeTemplateSyntax {
        TRACKING.with(|tracking| {
            let mut tracking = tracking.borrow_mut();
            std::mem::take(&mut tracking.as_mut().expect("active runtime trace").syntax)
        })
    }
}

impl Drop for Trace {
    /// Restores tracking state if the parser is reused or unwinds.
    fn drop(&mut self) {
        TRACKING.with(|tracking| tracking.replace(self.0.take()));
    }
}

/// Keeps generic parameters active until their declaration has finished parsing.
pub(in crate::syntax::parser) struct TypeScope(Option<usize>);

impl TypeScope {
    /// Remembers the outer declaration's type parameters before reading a header.
    pub(in crate::syntax::parser) fn enter() -> Self {
        if !super::runtime_fragment() {
            return Self(None);
        }
        Self(TRACKING.with(|tracking| tracking.borrow().as_ref().map(|value| value.parameters.len())))
    }
}

impl Drop for TypeScope {
    /// Removes only the generic parameters introduced by this declaration.
    fn drop(&mut self) {
        if let Some(outer) = self.0 {
            TRACKING.with(|tracking| {
                if let Some(tracking) = tracking.borrow_mut().as_mut() {
                    tracking.parameters.truncate(outer);
                }
            });
        }
    }
}

/// Records a parameter before reading its bound and default.
pub(in crate::syntax::parser) fn declare_parameter(name: &str, span: Span) {
    TRACKING.with(|tracking| {
        if let Some(tracking) = tracking.borrow_mut().as_mut() {
            tracking.parameters.push((name.to_owned(), span));
        }
    });
}

/// Records the source extent of a successfully parsed generic header.
pub(in crate::syntax::parser) fn record_header(header: RuntimeTemplateHeader) {
    TRACKING.with(|tracking| {
        if let Some(tracking) = tracking.borrow_mut().as_mut() {
            tracking.syntax.headers.push(header);
        }
    });
}

/// Records a name in a committed type or class-selection position.
pub(in crate::syntax::parser) fn record_reference(name: &Name, usage: Span) {
    if !super::runtime_fragment() || !name.is_unqualified() {
        return;
    }
    TRACKING.with(|tracking| {
        if let Some(tracking) = tracking.borrow_mut().as_mut() {
            if let Some((_, parameter)) = tracking.parameters.iter().rev().find(|(slot, _)| slot == name.as_str()) {
                tracking.syntax.references.push(RuntimeTemplateReference { usage, parameter: *parameter });
            }
        }
    });
}

/// Discards references recorded by a type lookahead probe.
pub(in crate::syntax::parser) fn lookahead<T>(parse: impl FnOnce() -> T) -> T {
    if !super::runtime_fragment() {
        return parse();
    }
    let count = TRACKING.with(|tracking| tracking.borrow().as_ref().map(|trace| {
        (trace.syntax.references.len(), trace.syntax.extensions.len())
    }));
    let value = parse();
    if let Some((references, extensions)) = count {
        TRACKING.with(|tracking| {
            if let Some(tracking) = tracking.borrow_mut().as_mut() {
                tracking.syntax.references.truncate(references);
                tracking.syntax.extensions.truncate(extensions);
            }
        });
    }
    value
}

/// Records a written extension independently of its PHPDoc provenance.
pub(in crate::syntax::parser) fn record_extension(span: Span) {
    if !super::runtime_fragment() {
        return;
    }
    TRACKING.with(|tracking| {
        if let Some(tracking) = tracking.borrow_mut().as_mut() {
            tracking.syntax.extensions.push(span);
        }
    });
}
