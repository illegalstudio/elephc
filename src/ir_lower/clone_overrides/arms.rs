//! Purpose:
//! Decides what PHP 8.5 `clone()` does with ONE override key, given the clone's runtime class
//! and the invocation-site scope, and names the exact diagnostic when the write is refused.
//!
//! Called from:
//! - `super::plan_class` while grouping applicators, and `super::body` while synthesizing their
//!   statement chains.
//!
//! Key details:
//! - Both inputs matter. A private slot is selected by the SCOPE when the scope declares one and
//!   the clone is an instance of it, which is what makes a parent-private property reachable on a
//!   child object and what keeps two same-named private slots apart.
//! - A strict ancestor's private property is INVISIBLE by name from anywhere else, so it behaves
//!   as an undefined name (php-src keeps it under a mangled key) rather than as a refusal.
//! - An ACCESS refusal (`private`/`protected` read visibility) is not final: php hands the write
//!   to `__set()` when the class has one, exactly as a plain `$obj->priv = 1` does. A WRITE
//!   refusal, asymmetric `set` visibility or `readonly`, is final and `__set()` never sees it.
//!   Both halves were measured against php 8.5.10.
//! - `readonly` carries an implicit `protected(set)` write visibility, which is why php 8.5
//!   answers `Cannot modify protected(set) readonly property R::$ro from global scope` from
//!   outside and still allows the reinitialization from the declaring class or a descendant.

use std::collections::HashMap;

use crate::parser::ast::Visibility;
use crate::types::ClassInfo;

/// What the applicator emits for one override key.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum OverrideArm {
    /// Write through the clone's own receiver, so the clone's visible slot wins.
    AssignThis,
    /// Call the named class's scoped setter helper, selecting THAT class's private slot.
    AssignScoped {
        /// Class declaring the private slot the invocation scope selects.
        scope: String,
        /// Property name the helper writes.
        property: String,
    },
    /// Route the write to the class's `__set()` magic setter.
    MagicSet,
    /// Store into the receiver's dynamic-property hash under the runtime name.
    ///
    /// Creating a name the class never declared emits php 8.5's dynamic-property deprecation
    /// unless the class is exempt; the backend decides that from
    /// `ClassInfo::dynamic_property_creation_is_deprecated()`.
    DynamicAssign,
    /// Raise this catchable `Error` message instead of writing.
    Deny(String),
    /// Raise `Cannot create dynamic property <class>::$<runtime name>` for an unknown key.
    DenyDynamicCreation,
}

/// Resolves one override key against the clone's class and the invocation scope.
pub(super) fn resolve(
    classes: &HashMap<String, ClassInfo>,
    class_name: &str,
    class_info: &ClassInfo,
    scope: Option<&str>,
    property: &str,
) -> OverrideArm {
    if let Some(arm) = resolve_scope_private_shadow(classes, class_name, scope, property) {
        return arm;
    }
    if class_info.visible_property_index(property).is_none() {
        return undefined_arm(classes, class_name, class_info, scope);
    }
    let visibility = class_info
        .property_visibilities
        .get(property)
        .cloned()
        .unwrap_or(Visibility::Public);
    let declaring = class_info
        .property_declaring_classes
        .get(property)
        .cloned()
        .unwrap_or_else(|| class_name.to_string());
    match visibility {
        Visibility::Public => {}
        Visibility::Protected => {
            if !shares_hierarchy(classes, scope, &declaring) {
                return denied_access_arm(
                    classes,
                    class_info,
                    scope,
                    format!(
                        "Cannot access protected property {}::${}",
                        class_name, property
                    ),
                );
            }
        }
        Visibility::Private => {
            // A strict ancestor's private slot is not part of this class's by-name table.
            if declaring != class_name {
                return undefined_arm(classes, class_name, class_info, scope);
            }
            if scope != Some(class_name) {
                return denied_access_arm(
                    classes,
                    class_info,
                    scope,
                    format!(
                        "Cannot access private property {}::${}",
                        class_name, property
                    ),
                );
            }
        }
    }
    apply_set_visibility(
        classes,
        class_info,
        &declaring,
        scope,
        property,
        OverrideArm::AssignThis,
    )
}

/// Turns a read-visibility refusal into `__set()` when the class declares a usable one.
///
/// php-src only reports the access error when no magic setter can take the write, and
/// `clone()` reuses that path unchanged: `clone($m, ["privateProp" => 5])` on a class with
/// `__set()` calls `__set('privateProp', 5)` from global scope instead of throwing.
fn denied_access_arm(
    classes: &HashMap<String, ClassInfo>,
    class_info: &ClassInfo,
    scope: Option<&str>,
    message: String,
) -> OverrideArm {
    magic_set_arm(classes, class_info, scope).unwrap_or(OverrideArm::Deny(message))
}

/// Returns the scope-selected private slot arm when the scope owns a same-named private property.
fn resolve_scope_private_shadow(
    classes: &HashMap<String, ClassInfo>,
    class_name: &str,
    scope: Option<&str>,
    property: &str,
) -> Option<OverrideArm> {
    let scope_name = scope?;
    let scope_info = classes.get(scope_name)?;
    if !declares_private(scope_info, scope_name, property) {
        return None;
    }
    if scope_name != class_name && !is_subclass_of(classes, class_name, scope_name) {
        return None;
    }
    let accepted = if scope_name == class_name {
        OverrideArm::AssignThis
    } else {
        OverrideArm::AssignScoped {
            scope: scope_name.to_string(),
            property: property.to_string(),
        }
    };
    Some(apply_set_visibility(
        classes,
        scope_info,
        scope_name,
        scope,
        property,
        accepted,
    ))
}

/// Returns whether `class_info` itself declares `property` as private.
pub(super) fn declares_private(
    class_info: &ClassInfo,
    class_name: &str,
    property: &str,
) -> bool {
    crate::types::class_declares_private_property(class_info, class_name, property)
}

/// Applies PHP 8.4 asymmetric write visibility and readonly reinitialization scope rules.
fn apply_set_visibility(
    classes: &HashMap<String, ClassInfo>,
    owner: &ClassInfo,
    declaring: &str,
    scope: Option<&str>,
    property: &str,
    accepted: OverrideArm,
) -> OverrideArm {
    let readonly = owner.readonly_properties.contains(property) || owner.is_readonly_class;
    let set_visibility = owner
        .property_set_visibilities
        .get(property)
        .cloned()
        .or(readonly.then_some(Visibility::Protected));
    let Some(set_visibility) = set_visibility else {
        return accepted;
    };
    let allowed = match set_visibility {
        Visibility::Public => true,
        Visibility::Protected => shares_hierarchy(classes, scope, declaring),
        Visibility::Private => scope == Some(declaring),
    };
    if allowed {
        return accepted;
    }
    let label = match set_visibility {
        Visibility::Public => "public",
        Visibility::Protected => "protected",
        Visibility::Private => "private",
    };
    let readonly_word = if readonly { "readonly " } else { "" };
    // A write refusal is FINAL: php does not offer it to `__set()`, which is why a class with a
    // magic setter still answers `Cannot modify protected(set) readonly property …` here.
    OverrideArm::Deny(format!(
        "Cannot modify {}(set) {}property {}::${} from {}",
        label,
        readonly_word,
        declaring,
        property,
        scope_phrase(scope)
    ))
}

/// Returns the arm PHP takes when the key names no property visible on the clone's class.
pub(super) fn undefined_arm(
    classes: &HashMap<String, ClassInfo>,
    class_name: &str,
    class_info: &ClassInfo,
    scope: Option<&str>,
) -> OverrideArm {
    if let Some(arm) = magic_set_arm(classes, class_info, scope) {
        return arm;
    }
    if class_info.has_property_hash_storage()
        || crate::types::checker::builtin_stdclass::is_stdclass(class_name)
    {
        return OverrideArm::DynamicAssign;
    }
    // Every class a two-argument `clone()` can reach was given a property hash by
    // `crate::types::checker::clone_override_storage`, so an ordinary class takes the arm above
    // and deprecates the creation exactly as php 8.5 does. This arm is left for the classes that
    // reservation deliberately skips: a checker-injected builtin, a packed class and an enum all
    // own their physical layout together with the code that reads it.
    OverrideArm::DenyDynamicCreation
}

/// Returns the `__set()` arm when the class declares a magic setter this scope may call.
fn magic_set_arm(
    classes: &HashMap<String, ClassInfo>,
    class_info: &ClassInfo,
    scope: Option<&str>,
) -> Option<OverrideArm> {
    let key = crate::names::php_symbol_key("__set");
    if !class_info.methods.contains_key(&key) {
        return None;
    }
    let visibility = class_info
        .method_visibilities
        .get(&key)
        .cloned()
        .unwrap_or(Visibility::Public);
    let declaring = class_info
        .method_declaring_classes
        .get(&key)
        .cloned()
        .unwrap_or_default();
    let reachable = match visibility {
        Visibility::Public => true,
        Visibility::Protected => shares_hierarchy(classes, scope, &declaring),
        Visibility::Private => scope == Some(declaring.as_str()),
    };
    reachable.then_some(OverrideArm::MagicSet)
}

/// Names the invocation scope the way php-src's member-access diagnostics do.
pub(super) fn scope_phrase(scope: Option<&str>) -> String {
    scope.map_or_else(
        || "global scope".to_string(),
        |class| format!("scope {}", class),
    )
}

/// Returns whether `child` inherits from `ancestor` through the declared parent chain.
pub(super) fn is_subclass_of(
    classes: &HashMap<String, ClassInfo>,
    child: &str,
    ancestor: &str,
) -> bool {
    crate::types::class_inherits_from(classes, child, ancestor)
}

/// Returns php-src's `zend_check_protected` verdict: the scope is an ancestor OR a descendant.
pub(super) fn shares_hierarchy(
    classes: &HashMap<String, ClassInfo>,
    scope: Option<&str>,
    declaring: &str,
) -> bool {
    crate::types::scope_shares_class_hierarchy(classes, scope, declaring)
}
