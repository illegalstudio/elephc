use crate::parser::ast::{ClassMethod, ClassProperty, ExprKind, MagicConstant};
use crate::span::Span;

use super::walker::{walk_class_method, walk_class_property, Pass};
use super::TRAIT_CLASS_PLACEHOLDER;

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

struct TraitClassPass {
    class_name: String,
}

impl Pass for TraitClassPass {
    fn transform_magic(&self, _span: Span, mc: MagicConstant) -> ExprKind {
        ExprKind::MagicConstant(mc)
    }

    fn transform_string(&self, value: String) -> ExprKind {
        if value.contains(TRAIT_CLASS_PLACEHOLDER) {
            ExprKind::StringLiteral(value.replace(TRAIT_CLASS_PLACEHOLDER, &self.class_name))
        } else {
            ExprKind::StringLiteral(value)
        }
    }
}
