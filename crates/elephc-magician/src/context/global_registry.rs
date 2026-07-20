//! Purpose:
//! Manages native-frame called-class overrides and the process-global eval class snapshot.
//!
//! Called from:
//! - Native bridge entry points and per-context class-like synchronization.
//!
//! Key details:
//! - Overrides are thread-local guards; global declarations are mutex-protected outside tests.

use super::*;

/// Late-static override installed while eval dispatches into a generated/AOT frame.
#[derive(Clone)]
pub(super) struct NativeFrameCalledClassOverride {
    #[cfg_attr(test, allow(dead_code))]
    context: *mut ElephcEvalContext,
    frame_class: String,
    called_class: String,
}

/// Scoped guard that removes one native-frame called-class override on drop.
pub(crate) struct NativeFrameCalledClassOverrideGuard;

/// Installs a late-static called-class override for a generated/AOT frame call.
pub(crate) fn push_native_frame_called_class_override(
    context: *mut ElephcEvalContext,
    frame_class: &str,
    called_class: &str,
) -> NativeFrameCalledClassOverrideGuard {
    NATIVE_FRAME_CALLED_CLASS_OVERRIDES.with(|overrides| {
        overrides
            .borrow_mut()
            .push(NativeFrameCalledClassOverride {
                context,
                frame_class: frame_class.trim_start_matches('\\').to_string(),
                called_class: called_class.trim_start_matches('\\').to_string(),
            });
    });
    NativeFrameCalledClassOverrideGuard
}

impl Drop for NativeFrameCalledClassOverrideGuard {
    /// Removes the most recent native-frame called-class override.
    fn drop(&mut self) {
        NATIVE_FRAME_CALLED_CLASS_OVERRIDES.with(|overrides| {
            overrides.borrow_mut().pop();
        });
    }
}

/// Returns the active thread-local late-static override for one generated/AOT frame.
pub(super) fn native_frame_called_class_override(
    frame_class: &str,
    called_class: &str,
) -> Option<String> {
    let frame_class = frame_class.trim_start_matches('\\');
    let called_class = called_class.trim_start_matches('\\');
    if frame_class.is_empty() || !called_class.eq_ignore_ascii_case(frame_class) {
        return None;
    }
    native_frame_called_class_override_for_frame(frame_class)
}

/// Returns the active called-class override for one generated/AOT frame class.
pub(crate) fn native_frame_called_class_override_for_frame(frame_class: &str) -> Option<String> {
    let frame_class = frame_class.trim_start_matches('\\');
    if frame_class.is_empty() {
        return None;
    }
    NATIVE_FRAME_CALLED_CLASS_OVERRIDES.with(|overrides| {
        overrides
            .borrow()
            .iter()
            .rev()
            .find(|entry| entry.frame_class.eq_ignore_ascii_case(frame_class))
            .map(|entry| entry.called_class.clone())
    })
}

/// Returns the active called-class override bytes for one generated/AOT frame class.
pub(crate) fn native_frame_called_class_override_bytes(
    frame_class: &str,
) -> Option<(*const u8, usize)> {
    let frame_class = frame_class.trim_start_matches('\\');
    if frame_class.is_empty() {
        return None;
    }
    NATIVE_FRAME_CALLED_CLASS_OVERRIDES.with(|overrides| {
        overrides
            .borrow()
            .iter()
            .rev()
            .find(|entry| entry.frame_class.eq_ignore_ascii_case(frame_class))
            .map(|entry| (entry.called_class.as_ptr(), entry.called_class.len()))
    })
}

/// Returns the active eval context and called class for one generated/AOT frame.
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn native_frame_called_class_override_context(
    frame_class: &str,
) -> Option<(*mut ElephcEvalContext, String)> {
    let frame_class = frame_class.trim_start_matches('\\');
    if frame_class.is_empty() {
        return None;
    }
    NATIVE_FRAME_CALLED_CLASS_OVERRIDES.with(|overrides| {
        overrides
            .borrow()
            .iter()
            .rev()
            .find(|entry| entry.frame_class.eq_ignore_ascii_case(frame_class))
            .map(|entry| (entry.context, entry.called_class.clone()))
    })
}

#[cfg(not(test))]
#[derive(Default)]
pub(super) struct GlobalEvalClassRegistry {
    pub(super) classes: HashMap<String, EvalClass>,
    pub(super) declared_class_names: Vec<String>,
    pub(super) interfaces: HashMap<String, EvalInterface>,
    pub(super) declared_interface_names: Vec<String>,
    pub(super) traits: HashMap<String, EvalTrait>,
    pub(super) declared_trait_names: Vec<String>,
    pub(super) enums: HashMap<String, EvalEnum>,
    pub(super) declared_enum_names: Vec<String>,
    pub(super) aliases: HashMap<String, EvalClassAlias>,
}

/// Returns the process-local eval class registry for generated-code eval contexts.
#[cfg(not(test))]
pub(super) fn global_eval_classes() -> &'static Mutex<GlobalEvalClassRegistry> {
    GLOBAL_EVAL_CLASSES.get_or_init(|| Mutex::new(GlobalEvalClassRegistry::default()))
}

/// Records one eval-declared class so later eval contexts can see PHP-global metadata.
#[cfg(not(test))]
pub(super) fn register_global_eval_class(class: &EvalClass) {
    let key = normalize_class_name(class.name());
    if let Ok(mut registry) = global_eval_classes().lock() {
        if !registry.classes.contains_key(&key) {
            registry.declared_class_names.push(class.name().to_string());
        }
        registry.classes.insert(key, class.clone());
    }
}

/// Records one eval-declared interface so later eval contexts can see PHP-global metadata.
#[cfg(not(test))]
pub(super) fn register_global_eval_interface(interface: &EvalInterface) {
    let key = normalize_class_name(interface.name());
    if let Ok(mut registry) = global_eval_classes().lock() {
        if !registry.interfaces.contains_key(&key) {
            registry
                .declared_interface_names
                .push(interface.name().to_string());
        }
        registry.interfaces.insert(key, interface.clone());
    }
}

/// Records one eval-declared trait so later eval contexts can see PHP-global metadata.
#[cfg(not(test))]
pub(super) fn register_global_eval_trait(trait_decl: &EvalTrait) {
    let key = normalize_class_name(trait_decl.name());
    if let Ok(mut registry) = global_eval_classes().lock() {
        if !registry.traits.contains_key(&key) {
            registry
                .declared_trait_names
                .push(trait_decl.name().to_string());
        }
        registry.traits.insert(key, trait_decl.clone());
    }
}

/// Records one eval-declared enum so later eval contexts can see PHP-global metadata.
#[cfg(not(test))]
pub(super) fn register_global_eval_enum(enum_decl: &EvalEnum) {
    let key = normalize_class_name(enum_decl.name());
    if let Ok(mut registry) = global_eval_classes().lock() {
        if !registry.enums.contains_key(&key) {
            registry
                .declared_enum_names
                .push(enum_decl.name().trim_start_matches('\\').to_string());
        }
        registry.enums.insert(key, enum_decl.clone());
    }
}

/// Records one eval-defined class-like alias for later generated eval contexts.
#[cfg(not(test))]
pub(super) fn register_global_eval_alias(alias_name: &str, alias: &EvalClassAlias) {
    let key = normalize_class_name(alias_name);
    if let Ok(mut registry) = global_eval_classes().lock() {
        registry.aliases.insert(key, alias.clone());
    }
}
