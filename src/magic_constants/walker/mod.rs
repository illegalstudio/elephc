//! Purpose:
//! Defines the reusable AST walker contract for magic-constant substitution passes.
//! Provides pass hooks for magic constants and scope entry/exit events.
//!
//! Called from:
//! - `crate::magic_constants::file_pass`, `scope_pass`, and `trait_binding`.
//!
//! Key details:
//! - Walkers rebuild AST nodes while preserving spans and delegating context-specific substitutions to `Pass`.

mod exprs;
mod members;
mod stmts;

use crate::names::Name;
use crate::parser::ast::{ExprKind, MagicConstant};
use crate::span::Span;

pub(super) use members::{walk_class_method, walk_class_property};
pub(super) use stmts::walk_program;

pub(super) trait Pass {
    /// Transforms a magic constant node (e.g., `__FILE__`, `__LINE__`) into its
    /// substituted expression. Called for every `MagicConstant` encountered during the walk.
    fn transform_magic(&self, span: Span, mc: MagicConstant) -> ExprKind;
    /// Transforms a string literal encountered in a position where a magic constant
    /// might appear (e.g., as the argument to `define`). Default returns the string unchanged.
    fn transform_string(&self, value: String) -> ExprKind {
        ExprKind::StringLiteral(value)
    }

    /// Called when entering a `namespace` declaration (the `namespace Foo;` statement).
    fn enter_namespace_decl(&mut self, _name: &Option<Name>) {}
    /// Called when entering a namespace block scope.
    fn enter_namespace_block(&mut self, _name: &Option<Name>) {}
    /// Called when leaving a namespace block scope.
    fn leave_namespace_block(&mut self) {}
    /// Called when entering a top-level function definition.
    fn enter_function(&mut self, _name: &str) {}
    /// Called when leaving a top-level function definition.
    fn leave_function(&mut self) {}
    /// Called when entering a class or interface declaration.
    fn enter_class(&mut self, _name: &str) {}
    /// Called when leaving a class or interface declaration.
    fn leave_class(&mut self) {}
    /// Called when entering a trait declaration.
    fn enter_trait(&mut self, _name: &str) {}
    /// Called when leaving a trait declaration.
    fn leave_trait(&mut self) {}
    /// Called when entering a method (class/trait/interface method).
    fn enter_method(&mut self, _name: &str) {}
    /// Called when leaving a method (class/trait/interface method).
    fn leave_method(&mut self) {}
    /// Called when entering a closure definition.
    fn enter_closure(&mut self, _span: Span) {}
    /// Called when leaving a closure definition.
    fn leave_closure(&mut self) {}
}
