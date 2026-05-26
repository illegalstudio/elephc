//! Purpose:
//! Rebinds trait-origin `__CLASS__` magic constant placeholders to the concrete class name.
//! Walks trait properties and methods after they are being applied to a class.
//!
//! Called from:
//! - `crate::magic_constants::bind_trait_class_constants()`.
//!
//! Key details:
//! - `__METHOD__` and `__TRAIT__` keep trait identity while only `__CLASS__` is rebound.

use crate::parser::ast::{ClassMethod, ClassProperty, ExprKind, MagicConstant};
use crate::span::Span;

use super::walker::{walk_class_method, walk_class_property, Pass};
use super::TRAIT_CLASS_PLACEHOLDER;

/// Rebinds `__CLASS__` magic constant placeholders in trait members to the concrete class name.
///
/// Walks trait properties and methods after they are applied to a class, replacing the
/// `TRAIT_CLASS_PLACEHOLDER` marker in string literals with `class_name`. Magic constants
/// themselves (`__CLASS__`, `__METHOD__`, `__TRAIT__`) are passed through unchanged;
/// only the placeholder text inside string literals is substituted.
pub(super) fn bind_trait_class_constants(
    properties: Vec<ClassProperty>,
    methods: Vec<ClassMethod>,
    class_name: &str,
) -> (Vec<ClassProperty>, Vec<ClassMethod>) {
    let mut pass = TraitClassPass {
        class_name: class_name.to_string(),
    };
    let properties = properties
        .into_iter()
        .map(|property| walk_class_property(property, &mut pass))
        .collect();
    let methods = methods
        .into_iter()
        .map(|method| walk_class_method(method, &mut pass))
        .collect();
    (properties, methods)
}

/// State carried through the trait-member rebinding pass.
struct TraitClassPass {
    /// The concrete class name to substitute for the placeholder.
    class_name: String,
}

impl Pass for TraitClassPass {
    /// Passes magic constants through without modification.
    fn transform_magic(&self, _span: Span, mc: MagicConstant) -> ExprKind {
        ExprKind::MagicConstant(mc)
    }

    /// Substitutes the `TRAIT_CLASS_PLACEHOLDER` marker in string literals with `class_name`.
    ///
    /// Returns an unmodified `ExprKind::StringLiteral` if the placeholder is absent.
    fn transform_string(&self, value: String) -> ExprKind {
        if value.contains(TRAIT_CLASS_PLACEHOLDER) {
            ExprKind::StringLiteral(value.replace(TRAIT_CLASS_PLACEHOLDER, &self.class_name))
        } else {
            ExprKind::StringLiteral(value)
        }
    }
}
