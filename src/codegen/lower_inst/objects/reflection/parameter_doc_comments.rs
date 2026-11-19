//! Purpose:
//! Extracts PHP 8.6 parameter doc comments from the synthetic parameter attribute channel.
//!
//! Called from:
//! - `default_members` when building `ReflectionParameter` metadata.
//!
//! Key details:
//! - The parser stores the raw `/** ... */` text as `#[\__elephc_param_doc("...")]`.
//! - This attribute is internal: it is removed from the attribute list that
//!   `ReflectionParameter::getAttributes()` exposes.

use crate::names::php_symbol_key;
use crate::parser::ast::{AttributeGroup, ExprKind};

/// Returns the raw doc comment carried by the synthetic parameter attribute.
pub(super) fn parameter_doc_comment(groups: &[AttributeGroup]) -> Option<String> {
    groups
        .iter()
        .flat_map(|group| &group.attributes)
        .find_map(|attribute| {
            if php_symbol_key(attribute.name.as_str()) != "__elephc_param_doc" {
                return None;
            }
            attribute.args.first().and_then(|arg| match &arg.kind {
                ExprKind::StringLiteral(value) => Some(value.clone()),
                _ => None,
            })
        })
}

/// Returns the non-internal attribute groups a caller may observe.
pub(super) fn visible_parameter_attributes(groups: &[AttributeGroup]) -> Vec<AttributeGroup> {
    groups
        .iter()
        .filter_map(|group| {
            let attributes: Vec<_> = group
                .attributes
                .iter()
                .filter(|attribute| {
                    php_symbol_key(attribute.name.as_str()) != "__elephc_param_doc"
                })
                .cloned()
                .collect();
            (!attributes.is_empty()).then(|| AttributeGroup {
                attributes,
                span: group.span,
            })
        })
        .collect()
}
