//! Purpose:
//! Classifies PHP static-call syntax that legally binds the current instance receiver.
//! Keeps checker, EIR lowering, and backend dispatch on one inheritance rule.
//!
//! Called from:
//! - Static method checking and AOT static-method lowering.
//!
//! Key details:
//! - A named or lexical receiver may invoke an instance method only when `$this` is compatible.
//! - `static::` needs late-bound dispatch and is intentionally handled by its dedicated lowering.

use std::collections::HashMap;

use crate::names::php_symbol_key;
use crate::parser::ast::StaticReceiver;

use super::ClassInfo;

/// Returns the explicit receiver class when static-call syntax can bind the current `$this`.
pub(crate) fn static_syntax_instance_receiver_class(
    classes: &HashMap<String, ClassInfo>,
    current_class: Option<&str>,
    receiver: &StaticReceiver,
) -> Option<String> {
    let current_class = current_class?.trim_start_matches('\\');
    let receiver_class = match receiver {
        StaticReceiver::Named(class_name) => class_name.as_str().trim_start_matches('\\').to_string(),
        StaticReceiver::Self_ => current_class.to_string(),
        StaticReceiver::Parent => classes
            .get(current_class)
            .and_then(|class_info| class_info.parent.clone())?,
        // `static::` resolves its instance method through late-bound dispatch rather than the
        // lexical direct-call path represented by this helper.
        StaticReceiver::Static => return None,
    };
    class_is_same_or_descends_from(classes, current_class, &receiver_class)
        .then_some(receiver_class)
}

/// Returns whether `class_name` is `base_class` or one of its descendants.
pub(crate) fn class_is_same_or_descends_from(
    classes: &HashMap<String, ClassInfo>,
    class_name: &str,
    base_class: &str,
) -> bool {
    let base_key = php_symbol_key(base_class.trim_start_matches('\\'));
    let mut current = Some(class_name.trim_start_matches('\\'));
    while let Some(name) = current {
        if php_symbol_key(name.trim_start_matches('\\')) == base_key {
            return true;
        }
        current = classes
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
}

/// Returns whether `static::` must dispatch an instance method through the late-bound `$this`.
pub(crate) fn static_syntax_uses_late_bound_instance_receiver(
    current_class: Option<&str>,
    receiver: &StaticReceiver,
) -> bool {
    current_class.is_some() && matches!(receiver, StaticReceiver::Static)
}
