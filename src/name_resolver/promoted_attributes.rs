//! Purpose:
//! Splits a promoted constructor parameter's predefined Core attributes between the parameter
//! and the property it declares, as php-src's compiler does.
//!
//! Called from:
//! - `crate::name_resolver::declarations` after a class or trait's members are resolved.
//!
//! Key details:
//! - The parser copies a promoted parameter's attributes onto both owners. php-src admits a Core
//!   attribute there when it matches either the parameter or the property target, then keeps it
//!   only on the owner it matches: `#[SensitiveParameter]` stays on the parameter alone,
//!   `#[Override]` on the property alone. One matching neither target stays on both, so the
//!   checker reports it against the parameter.
//! - Names must be resolved first: in a namespace, `SensitiveParameter` without an import names
//!   a user class, which is not split. User attributes stay on both owners.
//! - Only this class's own constructor parameters are promoted; an inherited promoted property
//!   of the same name does not make a child's plain parameter promoted.

use crate::parser::ast::{AttributeGroup, ClassMethod, ClassProperty};

const TARGET_PROPERTY: i64 = 8;
const TARGET_PARAMETER: i64 = 32;

/// Drops each promoted parameter's Core attributes from the owner whose target they do not
/// match: the property keeps the property-only ones, the parameter the parameter-only ones.
pub(super) fn split(
    mut properties: Vec<ClassProperty>,
    mut methods: Vec<ClassMethod>,
) -> (Vec<ClassProperty>, Vec<ClassMethod>) {
    let Some(constructor) = methods
        .iter_mut()
        .find(|method| method.name.eq_ignore_ascii_case("__construct"))
    else {
        return (properties, methods);
    };
    for property in properties.iter_mut().filter(|property| property.is_promoted) {
        let Some(index) = constructor.params.iter().position(|param| param.0 == property.name) else {
            continue;
        };
        property.attributes = without_other_target(&property.attributes, TARGET_PROPERTY, TARGET_PARAMETER);
        if let Some(attributes) = constructor.param_attributes.get_mut(index) {
            *attributes = without_other_target(attributes, TARGET_PARAMETER, TARGET_PROPERTY);
        }
    }
    (properties, methods)
}

/// Returns `groups` without the Core attributes that cannot target `own` but can target
/// `other`; a group left empty is dropped.
fn without_other_target(groups: &[AttributeGroup], own: i64, other: i64) -> Vec<AttributeGroup> {
    groups
        .iter()
        .map(|group| AttributeGroup {
            attributes: group
                .attributes
                .iter()
                .filter(|attribute| {
                    elephc_builtin_contract::core_attribute_flags(&attribute.name.as_canonical())
                        .is_none_or(|flags| flags & own != 0 || flags & other == 0)
                })
                .cloned()
                .collect(),
            span: group.span,
        })
        .filter(|group| !group.attributes.is_empty())
        .collect()
}
