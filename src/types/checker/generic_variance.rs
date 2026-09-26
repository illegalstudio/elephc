//! Purpose:
//! Decides whether one instantiation may be used where another is expected, when the template
//! between them declared a variance marker.
//!
//! Called from:
//! - `Checker::type_accepts`, as the last disjunct of its object case.
//!
//! Key details:
//! - The relation is checker-side and ONLY checker-side. `Box<Dog>` is not made to extend
//!   `Box<Animal>`: an inheritance edge shares static storage between the two classes, and each
//!   instantiation is required to keep its own.
//! - A widened receiver finds its method at the slot the STATIC type computed, so two
//!   instantiations have to number their vtable slots identically. Being spliced from one
//!   template body is not enough on its own: pruning used to recompact each class independently
//!   and pulled them apart, which `reachability/reconcile.rs` now prevents for instantiated
//!   classes. `.plans/generics.md` records how that was found.
//! - Widening is admitted only between two OBJECT arguments. That is the storage proof
//!   monomorphization forces and erased implementations never need: two objects are two
//!   pointers, so the widened value is the same bytes, while `Box<int>` to `Box<mixed>` would
//!   have to materialize a boxed cell — a copy, which is a different object.

use crate::generics::classes::template_key;
use crate::generics::instantiated_type;
use crate::parser::ast::{TypeExpr, Variance};
use crate::types::PhpType;

use super::Checker;

/// Whether a type argument is an object, and so shares storage with every other object.
fn is_object_argument(ty: &TypeExpr) -> bool {
    matches!(ty, TypeExpr::Named(_) | TypeExpr::GenericClass { .. })
}

/// The marker's name, for the hint.
fn variance_noun(variance: Variance) -> &'static str {
    match variance {
        Variance::Invariant => "invariant",
        Variance::Covariant => "covariant",
        Variance::Contravariant => "contravariant",
    }
}

impl Checker {
    /// Returns whether a value of class `actual_name` may be used where `expected_name` is
    /// expected, on the strength of a variance marker.
    ///
    /// Answers `false` for everything else, including two unrelated classes: this is one
    /// disjunct of `type_accepts`, never the whole answer.
    pub(crate) fn variance_permits(&self, expected_name: &str, actual_name: &str) -> bool {
        // Both names must be instantiations, and some template must actually declare a marker.
        // The ordinary program has no markers at all and pays only these two scans.
        if !expected_name.contains('<') || !actual_name.contains('<') {
            return false;
        }
        if !self.any_template_declares_variance() {
            return false;
        }
        let (Some(expected), Some(actual)) = (
            instantiated_type(expected_name),
            instantiated_type(actual_name),
        ) else {
            return false;
        };
        self.argument_widens(&expected, &actual)
    }

    /// Explains a refused widening between two instantiations of one marked template.
    ///
    /// Without this the diagnostic is `expects Box<mixed>, got Box<int>`, which reads as though
    /// the marker had been ignored. It was honoured; the storage is what refused. Saying so is
    /// the difference between "the compiler is wrong" and "widen at the declaration instead".
    pub(crate) fn variance_refusal_hint(
        &self,
        expected: &PhpType,
        actual: &PhpType,
    ) -> Option<String> {
        let (PhpType::Object(expected_name), PhpType::Object(actual_name)) = (expected, actual)
        else {
            return None;
        };
        if !expected_name.contains('<') || !actual_name.contains('<') {
            return None;
        }
        if !self.any_template_declares_variance() {
            return None;
        }
        let (
            Some(TypeExpr::GenericClass {
                name: expected_template,
                args: expected_args,
            }),
            Some(TypeExpr::GenericClass {
                name: actual_template,
                args: actual_args,
            }),
        ) = (
            instantiated_type(expected_name),
            instantiated_type(actual_name),
        )
        else {
            return None;
        };
        let key = template_key(expected_template.as_str());
        if key != template_key(actual_template.as_str()) || expected_args.len() != actual_args.len()
        {
            return None;
        }
        let signature = self
            .class_templates
            .iter()
            .find(|signature| signature.key == key)?;
        for (param, (expected_arg, actual_arg)) in signature
            .type_params
            .iter()
            .zip(expected_args.iter().zip(actual_args.iter()))
        {
            if param.variance == Variance::Invariant || expected_arg == actual_arg {
                continue;
            }
            // Two objects are two pointers, so a marked slot that still refuses them is a
            // subtyping failure and the plain message already says which classes. A slot whose
            // arguments are not both objects refused on STORAGE, which the message cannot show.
            if !is_object_argument(expected_arg) || !is_object_argument(actual_arg) {
                return Some(format!(
                    "'{}{}' is {}, but a widening has to be the same bytes, and these two type arguments \
                     do not share storage — only two object types can widen, because both are \
                     pointers",
                    param.variance.marker(),
                    param.name,
                    variance_noun(param.variance),
                ));
            }
        }
        None
    }

    /// Whether any collected template carries a marker, which gates the whole relation.
    fn any_template_declares_variance(&self) -> bool {
        self.class_templates.iter().any(|signature| {
            signature
                .type_params
                .iter()
                .any(|param| param.variance != Variance::Invariant)
        })
    }

    /// Whether a `from` value may be used where `to` is expected, with identical storage.
    fn argument_widens(&self, to: &TypeExpr, from: &TypeExpr) -> bool {
        match (to, from) {
            (TypeExpr::Named(to_name), TypeExpr::Named(from_name)) => {
                let (to_name, from_name) = (to_name.as_str(), from_name.as_str());
                to_name.eq_ignore_ascii_case(from_name)
                    || self.is_subclass_of(from_name, to_name)
                    || self.class_implements_interface(from_name, to_name)
                    || self.interface_extends_interface(from_name, to_name)
            }
            // A nested instantiation is itself an object, so the rule applies one level down:
            // `Box<Box<Dog>>` widens to `Box<Box<Animal>>` exactly when the inner one does.
            (
                TypeExpr::GenericClass {
                    name: to_name,
                    args: to_args,
                },
                TypeExpr::GenericClass {
                    name: from_name,
                    args: from_args,
                },
            ) => {
                template_key(to_name.as_str()) == template_key(from_name.as_str())
                    && to_args.len() == from_args.len()
                    && self.slots_widen(to_name.as_str(), to_args, from_args)
            }
            // THE STORAGE PROOF. Anything that is not an object on both sides is refused, even
            // when one is assignable to the other: `int` is a register and `mixed` is a boxed
            // tagged cell, so widening them would need a copy rather than the same bytes.
            _ => false,
        }
    }

    /// Whether every type argument of one template agrees under that slot's declared variance.
    fn slots_widen(&self, template: &str, to_args: &[TypeExpr], from_args: &[TypeExpr]) -> bool {
        let key = template_key(template);
        let Some(signature) = self
            .class_templates
            .iter()
            .find(|signature| signature.key == key)
        else {
            // A template the checker never saw declares no marker, so its instantiations stay
            // invariant — which the equality check in `type_accepts` has already answered.
            return false;
        };
        if signature.type_params.len() != to_args.len() {
            return false;
        }
        signature
            .type_params
            .iter()
            .zip(to_args.iter().zip(from_args.iter()))
            .all(|(param, (to, from))| match param.variance {
                Variance::Invariant => to == from,
                Variance::Covariant => self.argument_widens(to, from),
                Variance::Contravariant => self.argument_widens(from, to),
            })
    }
}
