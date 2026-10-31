//! Purpose:
//! Manages native-frame called-class overrides and the process-global eval class snapshot.
//!
//! Called from:
//! - Native bridge entry points and per-context class-like synchronization.
//!
//! Key details:
//! - Overrides are thread-local guards; global declarations are mutex-protected outside tests.

use super::*;

thread_local! {
    /// Stable ABI handles, removed before their owning contexts are reclaimed.
    static LIVE_NATIVE_CONTEXTS: RefCell<Vec<*mut ElephcEvalContext>> = const { RefCell::new(Vec::new()) };
}

/// Publishes a heap context after all initialization is complete and its address is stable.
pub(crate) fn register_live_native_context(context: *mut ElephcEvalContext) {
    LIVE_NATIVE_CONTEXTS.with(|contexts| contexts.borrow_mut().push(context));
}

/// Removes a context before any owner or callback can observe reclaimed metadata.
pub(crate) fn unregister_live_native_context(context: *mut ElephcEvalContext) {
    LIVE_NATIVE_CONTEXTS.with(|contexts| contexts.borrow_mut().retain(|entry| *entry != context));
}

/// Finds a live request owner for class-based factories called after an eval activation returns.
///
/// # Safety
/// Context handles must be registered and removed on their owning thread before reclamation.
pub(crate) unsafe fn live_native_context_for_class(name: &str) -> Option<*mut ElephcEvalContext> {
    let contexts = LIVE_NATIVE_CONTEXTS.with(|contexts| contexts.borrow().clone());
    for context in contexts {
        let Some(owner) = (unsafe { context.as_mut() }) else { continue; };
        if !owner.has_abi_owners() || owner.teardown_active() { continue; }
        #[cfg(not(test))]
        owner.sync_global_eval_classes();
        if owner.resolve_class_like_name(name).is_some() { return Some(context); }
    }
    None
}

/// Tests eval class-like names against the target followed by its compiled positive candidates.
pub(crate) fn native_class_string_relation(
    context: &ElephcEvalContext, source: &str, candidates: &[&str], exclude_self: bool,
) -> bool {
    let Some(source) = context.resolve_class_like_name(source) else { return false; };
    let Some(target) = candidates.first() else { return false; };
    let target = context.resolve_class_like_name(target).unwrap_or_else(|| target.trim_start_matches('\\').into());
    if exclude_self && source.eq_ignore_ascii_case(&target) { return false; }
    let interface_parents = context.interface_parent_names(&source);
    candidates.iter().any(|candidate| {
        let candidate = candidate.trim_start_matches('\\');
        source.eq_ignore_ascii_case(candidate)
            || context.class_is_a(&source, candidate, false)
            || interface_parents.iter().any(|parent| parent.eq_ignore_ascii_case(candidate))
    })
}

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

/// Keeps an interpreter context active across a native call without changing class resolution.
pub(crate) fn push_native_frame_context(
    context: *mut ElephcEvalContext,
) -> NativeFrameCalledClassOverrideGuard {
    push_native_frame_called_class_override(context, "", "")
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

/// Borrows the active eval context while an interpreter call is inside generated code.
///
/// Prelude factories can cross ordinary helper frames without an eval context local.
/// The owning interpreter frame keeps this context alive until the native call returns.
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn native_frame_active_eval_context() -> Option<*mut ElephcEvalContext> {
    NATIVE_FRAME_CALLED_CLASS_OVERRIDES.with(|overrides| {
        overrides.borrow().last().map(|entry| entry.context).filter(|context| !context.is_null())
    })
}

#[cfg(test)]
mod active_context_tests {
    use super::*;

    /// Factories reuse a live ABI owner and never return a handle after its final reclamation.
    #[test]
    fn class_factory_context_lookup_tracks_retained_abi_lifetimes() {
        let context = crate::ffi::__elephc_eval_context_new();
        unsafe {
            assert!((*context).define_class(EvalClass::new("LiveFactoryContextNode", vec![], vec![])));
            assert_eq!(live_native_context_for_class("\\livefactorycontextnode"), Some(context));
            assert_eq!(crate::ffi::__elephc_eval_context_retain(context), 0);
            crate::ffi::__elephc_eval_context_free(context);
            assert_eq!(live_native_context_for_class("LiveFactoryContextNode"), Some(context));
            crate::ffi::__elephc_eval_context_free(context);
            assert_eq!(live_native_context_for_class("LiveFactoryContextNode"), None);
        }
    }

    /// Compiled positive candidates include native ancestors implementing the requested interface.
    #[test]
    fn eval_class_string_relations_preserve_aliases_self_and_compiled_parents() {
        let mut context = ElephcEvalContext::new();
        assert!(context.define_class(EvalClass::with_relations("DynamicFactoryChild",
            Some("NativeFactoryParent".into()), vec![], vec![], vec![])));
        assert!(context.define_interface(EvalInterface::new("DynamicFactoryInterface",
            vec!["NativeFactoryInterface".into()], vec![])));
        assert!(context.define_class_alias("DynamicFactoryChild", "DynamicFactoryAlias"));
        assert!(native_class_string_relation(&context, "\\dynamicfactoryalias", &["NativeFactoryParent"], false));
        assert!(native_class_string_relation(&context, "DynamicFactoryChild", &["NativeFactoryParent"], true));
        assert!(native_class_string_relation(&context, "DynamicFactoryChild", &["Countable", "NativeFactoryParent"], false));
        assert!(native_class_string_relation(&context, "DynamicFactoryInterface", &["NativeFactoryInterface"], true));
        assert!(!native_class_string_relation(&context, "DynamicFactoryAlias", &["DynamicFactoryChild"], true));
        assert!(native_class_string_relation(&context, "DynamicFactoryAlias", &["DynamicFactoryChild"], false));
        assert!(!native_class_string_relation(&context, "MissingFactoryClass", &["MissingFactoryClass"], false));
    }

    /// Nested global calls restore their caller's context without masking called-class metadata.
    #[test]
    fn context_only_native_frames_preserve_class_overrides_and_restore_context() {
        let mut outer = ElephcEvalContext::new();
        let mut inner = ElephcEvalContext::new();
        let outer_ptr = &mut outer as *mut ElephcEvalContext;
        let inner_ptr = &mut inner as *mut ElephcEvalContext;
        {
            let _method = push_native_frame_called_class_override(outer_ptr, "Parent", "Child");
            assert_eq!(native_frame_active_eval_context(), Some(outer_ptr));
            {
                let _function = push_native_frame_context(inner_ptr);
                assert_eq!(native_frame_active_eval_context(), Some(inner_ptr));
                assert_eq!(native_frame_called_class_override_for_frame("Parent"), Some("Child".into()));
                assert_eq!(native_frame_called_class_override_context("Parent"), Some((outer_ptr, "Child".into())));
                assert_eq!(native_frame_called_class_override_for_frame(""), None);
            }
            assert_eq!(native_frame_active_eval_context(), Some(outer_ptr));
        }
        assert_eq!(native_frame_active_eval_context(), None);
    }
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
