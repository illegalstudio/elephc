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
pub(crate) use stmts::walk_program;

use crate::parser::ast::TypeExpr;

/// Routes one inheritance clause entry (`extends Box<int>`, `implements Repository<User>`)
/// through [`Pass::transform_type`], and puts the answer back in the shape the AST stores it.
///
/// An inherited name is a type position like any other — it is the only place a generic class
/// is named without a `TypeExpr` around it — so a pass that rewrites `Box<int>` to the
/// instantiated class has to see it here too, or a class would end up extending the TEMPLATE.
///
/// A pass that turns the clause into something that is not a class type has said nothing usable
/// about inheritance, so the original is kept: `extends` needs a name, not an `int`.
pub(super) fn walk_inherited<P: Pass>(
    name: Name,
    args: Vec<TypeExpr>,
    pass: &P,
    span: Span,
) -> (Name, Vec<TypeExpr>) {
    let written = if args.is_empty() {
        TypeExpr::Named(name.clone())
    } else {
        TypeExpr::GenericClass {
            name: name.clone(),
            args: args.clone(),
        }
    };
    match pass.transform_type(written, span) {
        TypeExpr::Named(resolved) => (resolved, Vec::new()),
        TypeExpr::GenericClass {
            name: resolved,
            args,
        } => (resolved, args),
        _ => (name, args),
    }
}

/// Routes a static receiver through [`Pass::transform_type`], collapsing it once it is concrete.
///
/// A receiver that is no longer generic is an ordinary named receiver, and collapsing it HERE is
/// what keeps `StaticReceiver::Generic` out of every later pass — the instantiating pass only
/// has to answer "what does this type become", never "and which receiver should replace it".
///
/// A pass that turns the receiver into something that is not a class type has said nothing
/// usable about it, so the original is kept: `::` needs a class, not an `int`.
pub(super) fn walk_static_receiver<P: Pass>(
    receiver: crate::parser::ast::StaticReceiver,
    pass: &P,
    span: Span,
) -> crate::parser::ast::StaticReceiver {
    use crate::parser::ast::StaticReceiver;
    let StaticReceiver::Generic(class_type) = receiver else {
        // A plain named receiver can still be renamed: `Box::of(5)` names a template until the
        // checker works out which instantiation it meant.
        if let StaticReceiver::Named(name) = receiver {
            return StaticReceiver::Named(pass.transform_class_reference(name, span));
        }
        return receiver;
    };
    match pass.transform_type(class_type, span) {
        TypeExpr::Named(name) => StaticReceiver::Named(name),
        class_type => StaticReceiver::Generic(class_type),
    }
}

/// Routes a list of inherited names and their type arguments through [`walk_inherited`].
///
/// `args` is padded to the length of `names` first: the two are aligned index by index, and a
/// declaration parsed before this field existed — or built by a synthetic-class pass — carries
/// a shorter list than its `implements`.
pub(super) fn walk_inherited_list<P: Pass>(
    names: Vec<Name>,
    mut args: Vec<Vec<TypeExpr>>,
    pass: &P,
    span: Span,
) -> (Vec<Name>, Vec<Vec<TypeExpr>>) {
    args.resize(names.len(), Vec::new());
    let mut walked_names = Vec::with_capacity(names.len());
    let mut walked_args = Vec::with_capacity(names.len());
    for (name, name_args) in names.into_iter().zip(args) {
        let (name, name_args) = walk_inherited(name, name_args, pass, span);
        walked_names.push(name);
        walked_args.push(name_args);
    }
    (walked_names, walked_args)
}

pub(crate) trait Pass {
    /// Transforms a magic constant node (e.g., `__FILE__`, `__LINE__`) into its
    /// substituted expression. Called for every `MagicConstant` encountered during the walk.
    fn transform_magic(&self, span: Span, mc: MagicConstant) -> ExprKind;
    /// Transforms a string literal encountered in a position where a magic constant
    /// might appear (e.g., as the argument to `define`). Default returns the string unchanged.
    fn transform_string(&self, value: String) -> ExprKind {
        ExprKind::StringLiteral(value)
    }
    /// Transforms a type annotation. Called for EVERY `TypeExpr` position the walk reaches —
    /// typed locals, `buffer<T>` element types, class property, constant and method
    /// annotations, the inheritance clauses' type arguments, and the parameter, variadic and
    /// return types of functions, methods and closures. Default returns it unchanged.
    ///
    /// The walk is exhaustive (neither statement nor expression match carries a wildcard arm),
    /// which is what makes this hook a complete substitution point rather than a best effort.
    ///
    /// `span` is the nearest enclosing declaration's span — the statement, the member, or the
    /// expression. A `TypeExpr` carries none of its own, and a pass that REJECTS a type (an
    /// unknown generic class, an arity mismatch) has to report it somewhere the programmer can
    /// find; `Span::dummy()` would lose the file, because include resolution splices every file
    /// into one program and the span is all that recovers which one a diagnostic came from.
    fn transform_type(
        &self,
        ty: crate::parser::ast::TypeExpr,
        _span: Span,
    ) -> crate::parser::ast::TypeExpr {
        ty
    }

    /// Transforms a class NAMED at a use site — a construction or a static receiver.
    ///
    /// Exists for one caller: `generics::classes` renaming `new Box(5)` and `Box::of(5)` to the
    /// class the checker inferred for each. A `new` and a `::` cannot share a span, so both go
    /// through one hook and one map. The default makes every other pass ignore it entirely.
    fn transform_class_reference(&self, class_name: Name, _span: Span) -> Name {
        class_name
    }

    /// Transforms the NAME of an instance method call.
    ///
    /// Exists for one caller: `generics::classes` renaming `$b->pickOr(1, 0)` to the
    /// instantiation the checker inferred for it, `pickOr<int>`. A generic method is a template
    /// and its instantiations are ordinary methods with distinct names, so the call has to name
    /// the one it selected — nothing downstream could work it out, because only the checker knows
    /// the argument types. The default makes every other pass ignore it entirely.
    fn transform_method_call_name(&self, method: String, _span: Span) -> String {
        method
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
    /// Called when entering a method, with the type parameters it declares of its own.
    ///
    /// The list is how a pass tells a generic METHOD from an ordinary one. `generics::classes`
    /// needs it: a `Box<U>` written inside `map<U>` names no class, and instantiating it would
    /// emit a class called `Box<U>` — the same trap a generic FUNCTION's body springs, which is
    /// why that one is guarded by an `in_template` counter too.
    fn enter_method(&mut self, _name: &str, _type_params: &[crate::parser::ast::TypeParam]) {}
    /// Called when leaving a method (class/trait/interface method).
    fn leave_method(&mut self) {}
    /// Called when entering a closure definition.
    fn enter_closure(&mut self, _span: Span) {}
    /// Called when leaving a closure definition.
    fn leave_closure(&mut self) {}
}
