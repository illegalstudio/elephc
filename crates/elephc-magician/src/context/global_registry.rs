//! Purpose:
//! Manages native-frame called-class overrides plus process-global eval class and include state.
//!
//! Called from:
//! - Native bridge entry points, include execution, and per-context class-like synchronization.
//!
//! Key details:
//! - Overrides are thread-local guards; global declarations and include keys are mutex-protected outside tests.

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

/// Records the lexical class of a compiled frame that is about to call into the bridge.
///
/// Paired with [`pop_native_caller_class`] around exactly one bridge call, so the stack depth
/// matches the native call depth even when a call nests.
pub(crate) fn push_native_caller_class(class_name: &str) {
    let class_name = class_name.trim_start_matches('\\').to_string();
    NATIVE_CALLER_CLASSES.with(|classes| classes.borrow_mut().push(class_name));
}

/// Drops the innermost compiled-frame lexical class.
pub(crate) fn pop_native_caller_class() {
    NATIVE_CALLER_CLASSES.with(|classes| {
        classes.borrow_mut().pop();
    });
}

/// Returns the lexical class of the innermost compiled frame calling into the bridge.
pub(crate) fn current_native_caller_class() -> Option<String> {
    NATIVE_CALLER_CLASSES.with(|classes| classes.borrow().last().cloned())
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
    /// Bumped whenever the registry is emptied, so a context can tell "nothing new since I
    /// last looked" from "everything I saw is gone and the indices mean something else now".
    pub(super) generation: u64,
    pub(super) classes: rustc_hash::FxHashMap<String, Arc<EvalClass>>,
    pub(super) class_source_files: HashMap<String, String>,
    pub(super) declared_class_names: Vec<String>,
    pub(super) interfaces: rustc_hash::FxHashMap<String, Arc<EvalInterface>>,
    pub(super) declared_interface_names: Vec<String>,
    pub(super) traits: rustc_hash::FxHashMap<String, Arc<EvalTrait>>,
    pub(super) declared_trait_names: Vec<String>,
    pub(super) enums: rustc_hash::FxHashMap<String, Arc<EvalEnum>>,
    pub(super) declared_enum_names: Vec<String>,
    /// Each eval enum case's singleton and backing value (raw cell addresses), keyed by the
    /// normalized enum and case names. A context imports an enum's DECLARATION through the sync
    /// above, but the case objects are made once, in the declaring context; without this a
    /// second context could see the enum and still not answer `Enum::CASE`.
    pub(super) enum_cases: HashMap<(String, String), (usize, Option<usize>)>,
    pub(super) aliases: rustc_hash::FxHashMap<String, EvalClassAlias>,
}

/// Immutable AOT metadata shared with fallback contexts created without a generated frame.
///
/// Dynamic class-like declarations stay in `GlobalEvalClassRegistry`; this snapshot contains
/// only compiler-provided declarations and signatures that are valid for the whole binary.
///
/// Every table is reference-counted rather than owned: the first generated context builds
/// them, publishes the handles, and each later context adopts the same allocations instead
/// of deep-copying a Symfony-sized registration stream per request. Contexts only ever read
/// a synced table; the registration setters go through `Arc::make_mut`, so a context that
/// registers after adopting the snapshot silently forks its own table and the published one
/// is never observed to change.
#[cfg(not(test))]
pub(super) struct GlobalEvalAotMetadata {
    native_global_sync: Option<NativeGlobalSyncHooks>,
    declared_class_names: Arc<Vec<String>>,
    declared_interface_names: Arc<Vec<String>>,
    declared_trait_names: Arc<Vec<String>>,
    native_functions: Arc<HashMap<String, NativeFunction>>,
    native_methods: Arc<HashMap<(String, String), NativeCallableSignature>>,
    native_static_methods: Arc<HashMap<(String, String), NativeCallableSignature>>,
    native_constructors: Arc<HashMap<String, NativeCallableSignature>>,
    native_class_parents: Arc<HashMap<String, String>>,
    native_class_attributes: Arc<HashMap<String, Vec<EvalAttribute>>>,
    native_method_attributes: Arc<HashMap<(String, String), Vec<EvalAttribute>>>,
    native_constant_attributes: Arc<HashMap<(String, String), Vec<EvalAttribute>>>,
    native_interface_properties: Arc<HashMap<String, Vec<(String, EvalInterfaceProperty)>>>,
    native_abstract_properties: Arc<HashMap<String, Vec<(String, EvalInterfaceProperty)>>>,
    native_property_types: Arc<HashMap<(String, String), EvalParameterType>>,
    native_property_defaults: Arc<HashMap<(String, String), NativeCallableDefault>>,
    native_property_default_errors: Arc<HashMap<(String, String), String>>,
    native_class_default_errors: Arc<HashMap<String, String>>,
    native_property_attributes: Arc<HashMap<(String, String), Vec<EvalAttribute>>>,
    native_class_constant_defaults: Arc<HashMap<(String, String), NativeClassConstantDefault>>,
    native_global_constants: Arc<HashMap<String, EvalNativeGlobalConstant>>,
    native_user_constants: Arc<HashMap<String, EvalNativeUserConstant>>,
}

/// Returns the process-local snapshot of generated AOT metadata.
#[cfg(not(test))]
fn global_eval_aot_metadata() -> &'static Mutex<Option<Arc<GlobalEvalAotMetadata>>> {
    GLOBAL_EVAL_AOT_METADATA.get_or_init(|| Mutex::new(None))
}

/// Publishes compiler-provided metadata after one generated context has finished registration.
#[cfg(not(test))]
pub(crate) fn publish_global_eval_aot_metadata(context: &mut ElephcEvalContext) {
    let Ok(mut metadata) = global_eval_aot_metadata().lock() else {
        return;
    };
    let declared_class_names = Arc::new(context.declared_class_names());
    let declared_interface_names = Arc::new(context.declared_interface_names());
    let declared_trait_names = Arc::new(context.declared_trait_names());
    *metadata = Some(Arc::new(GlobalEvalAotMetadata {
            native_global_sync: context.native_global_sync,
            // Flattened once, here: the snapshot is the prefix every later context adopts,
            // so it must carry the registering context's own additions too.
            declared_class_names,
            declared_interface_names,
            declared_trait_names,
            native_functions: context.native_functions.share(),
            native_methods: context.native_methods.share(),
            native_static_methods: context.native_static_methods.share(),
            native_constructors: context.native_constructors.share(),
            native_class_parents: context.native_class_parents.share(),
            native_class_attributes: context.native_class_attributes.share(),
            native_method_attributes: context.native_method_attributes.share(),
            native_constant_attributes: context.native_constant_attributes.share(),
            native_interface_properties: context.native_interface_properties.share(),
            native_abstract_properties: context.native_abstract_properties.share(),
            native_property_types: context.native_property_types.share(),
            native_property_defaults: context.native_property_defaults.share(),
            native_property_default_errors: context.native_property_default_errors.share(),
            native_class_default_errors: context.native_class_default_errors.share(),
            native_property_attributes: context.native_property_attributes.share(),
            native_class_constant_defaults: context.native_class_constant_defaults.share(),
            // The module registration also seeds the constant inventories; a context that
            // adopts this snapshot skips that registration, so they must travel with it.
            native_global_constants: context.native_global_constants.share(),
            native_user_constants: context.native_user_constants.share(),
    }));
}

/// Imports compiler-provided metadata into a newly created context.
///
/// Returns whether a process-global AOT snapshot was available. Generated
/// contexts use this to skip replaying the complete registration stream after
/// the first context has published immutable module metadata.
#[cfg(not(test))]
pub(crate) fn sync_global_eval_aot_metadata(context: &mut ElephcEvalContext) -> bool {
    let Some(metadata) = published_global_eval_aot_metadata()
        .or_else(publish_global_eval_aot_metadata_from_module)
    else {
        return false;
    };
    // Already holding this very snapshot: a recycled context, or the null-handle context on
    // its second call. Re-installing it would also clear the `own_*` name lists below, which
    // for such a context hold the names of classes still in its tables.
    if context.native_functions.shares_with(&metadata.native_functions)
        && context.declared_class_names.shares_with(&metadata.declared_class_names)
    {
        return true;
    }
    context.declared_class_names = SharedNative::from_shared(Arc::clone(&metadata.declared_class_names));
    context.own_declared_class_names.clear();
    context.native_global_sync = metadata.native_global_sync;
    context.declared_interface_names = SharedNative::from_shared(Arc::clone(&metadata.declared_interface_names));
    context.own_declared_interface_names.clear();
    context.declared_trait_names = SharedNative::from_shared(Arc::clone(&metadata.declared_trait_names));
    context.own_declared_trait_names.clear();
    context.native_functions = SharedNative::from_shared(Arc::clone(&metadata.native_functions));
    context.native_methods = SharedNative::from_shared(Arc::clone(&metadata.native_methods));
    context.native_static_methods = SharedNative::from_shared(Arc::clone(&metadata.native_static_methods));
    context.native_constructors = SharedNative::from_shared(Arc::clone(&metadata.native_constructors));
    context.native_class_parents = SharedNative::from_shared(Arc::clone(&metadata.native_class_parents));
    context.native_class_attributes = SharedNative::from_shared(Arc::clone(&metadata.native_class_attributes));
    context.native_method_attributes = SharedNative::from_shared(Arc::clone(&metadata.native_method_attributes));
    context.native_constant_attributes = SharedNative::from_shared(Arc::clone(&metadata.native_constant_attributes));
    context.native_interface_properties = SharedNative::from_shared(Arc::clone(&metadata.native_interface_properties));
    context.native_abstract_properties = SharedNative::from_shared(Arc::clone(&metadata.native_abstract_properties));
    context.native_property_types = SharedNative::from_shared(Arc::clone(&metadata.native_property_types));
    context.native_property_defaults = SharedNative::from_shared(Arc::clone(&metadata.native_property_defaults));
    context.native_property_default_errors = SharedNative::from_shared(Arc::clone(&metadata.native_property_default_errors));
    context.native_class_default_errors = SharedNative::from_shared(Arc::clone(&metadata.native_class_default_errors));
    context.native_property_attributes = SharedNative::from_shared(Arc::clone(&metadata.native_property_attributes));
    context.native_class_constant_defaults = SharedNative::from_shared(Arc::clone(&metadata.native_class_constant_defaults));
    context.native_global_constants = SharedNative::from_shared(Arc::clone(&metadata.native_global_constants));
    context.native_user_constants = SharedNative::from_shared(Arc::clone(&metadata.native_user_constants));
    true
}

/// Returns the AOT metadata snapshot, if the module's registration has published one.
#[cfg(not(test))]
fn published_global_eval_aot_metadata() -> Option<Arc<GlobalEvalAotMetadata>> {
    global_eval_aot_metadata().lock().ok().and_then(|metadata| metadata.clone())
}

#[cfg(not(test))]
unsafe extern "C" {
    /// Runs the generated module registration on `ctx`, which publishes the AOT snapshot.
    ///
    /// The context is opaque to generated code, so it crosses as an untyped pointer.
    fn __elephc_eval_register_module_metadata(ctx: *mut std::ffi::c_void);
}

/// Publishes the AOT snapshot by running the module registration on a scratch context.
///
/// Only that registration publishes, and generated code runs it when a body builds its own eval
/// context. A bridge call reached with a null handle before any such body -- a compiled
/// `new $class(...)` ahead of the first `eval()` -- found nothing to sync, so its context had no
/// constructor signature and the bridge refused the call. Attempted once per process: a module
/// with nothing to register stays without a snapshot, as it did before.
#[cfg(not(test))]
fn publish_global_eval_aot_metadata_from_module() -> Option<Arc<GlobalEvalAotMetadata>> {
    static ATTEMPTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if ATTEMPTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return None;
    }
    let scratch = crate::ffi::context::__elephc_eval_context_new();
    if scratch.is_null() {
        return None;
    }
    unsafe {
        __elephc_eval_register_module_metadata(scratch.cast());
        crate::ffi::context::__elephc_eval_context_free(scratch);
    }
    published_global_eval_aot_metadata()
}

/// Imports the module's published AOT metadata into a context that has never received any.
///
/// A context allocated before the generated module published its snapshot keeps an empty callable
/// table for the rest of the request, because the import happens once, at creation. An AOT
/// `spl_autoload_register()` produces exactly that: the ABI always hands it a null handle, so it
/// allocates the owner context on the spot, and the module's registration helper usually has not
/// run yet at that point. The snapshot is a module constant, so a context still holding nothing of
/// its own can take it later without overwriting anything.
///
/// Returns whether the metadata was imported by this call.
#[cfg(not(test))]
pub(crate) fn sync_global_eval_aot_metadata_when_empty(context: &mut ElephcEvalContext) -> bool {
    if !context.native_functions.is_empty()
        || !context.native_methods.is_empty()
        || !context.native_static_methods.is_empty()
        || !context.native_constructors.is_empty()
    {
        return false;
    }
    sync_global_eval_aot_metadata(context)
}

/// Returns the process-local eval class registry for generated-code eval contexts.
#[cfg(not(test))]
pub(super) fn global_eval_classes() -> &'static Mutex<GlobalEvalClassRegistry> {
    GLOBAL_EVAL_CLASSES.get_or_init(|| Mutex::new(GlobalEvalClassRegistry::default()))
}

/// Returns the process-local map from PHP function name to its owning eval context.
#[cfg(not(test))]
fn global_eval_functions() -> &'static Mutex<HashMap<String, usize>> {
    GLOBAL_EVAL_FUNCTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Returns the request-global owners that retain SPL autoload callback tables.
#[cfg(not(test))]
fn global_eval_autoload_contexts() -> &'static Mutex<Vec<usize>> {
    GLOBAL_EVAL_AUTOLOAD_CONTEXTS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Snapshots every live request-global SPL autoload context without holding its registry lock.
#[cfg(not(test))]
pub(crate) fn global_eval_autoload_contexts_snapshot() -> Vec<*mut ElephcEvalContext> {
    global_eval_autoload_contexts()
        .lock()
        .map(|contexts| {
            contexts
                .iter()
                .copied()
                .map(|context| context as *mut ElephcEvalContext)
                .collect()
        })
        .unwrap_or_default()
}

/// Clears request-scoped dynamic class-like declarations without discarding AOT metadata.
#[cfg(not(test))]
pub(crate) fn reset_global_eval_classes() {
    let mut registry = global_eval_classes()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // These are declarations executed by the previous request, not immutable AOT
    // metadata. Retaining them would make an ordinary include in the next request
    // fail with a redeclaration error. The separate AOT snapshot remains reusable.
    let generation = registry.generation.wrapping_add(1);
    *registry = GlobalEvalClassRegistry::default();
    // The names a context already imported are gone, and index 7 of the new lists is a
    // different class than index 7 of the old ones. Every sync mark must be invalidated.
    registry.generation = generation;
}

/// Clears the process-local include registry at a generated web request boundary.
#[cfg(not(test))]
pub(crate) fn reset_global_eval_included_files() {
    super::request_includes::current_include_state()
        .lock().unwrap_or_else(std::sync::PoisonError::into_inner).reset();
}

/// Releases request-scoped dynamic function owners before the web heap is recycled.
#[cfg(not(test))]
pub(crate) fn reset_global_eval_function_contexts() {
    let owners = {
        let mut functions = global_eval_functions()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut *functions)
            .into_values()
            .collect::<Vec<_>>()
    };
    for owner in owners {
        let context = owner as *mut ElephcEvalContext;
        let should_finalize = unsafe { context.as_ref() }
            .is_some_and(ElephcEvalContext::forget_global_function);
        if should_finalize {
            unsafe { crate::ffi::context::finalize_eval_context_free(context) };
        }
    }
}

/// Releases every request-global SPL autoload callback table before recycling the web heap.
#[cfg(not(test))]
pub(crate) fn reset_global_eval_autoload_contexts() {
    let contexts = {
        let mut contexts = global_eval_autoload_contexts()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut *contexts).into_iter().collect::<Vec<_>>()
    };
    for context in contexts {
        release_global_eval_autoload_context(context as *mut ElephcEvalContext);
    }
}

/// Keeps crate unit tests isolated without introducing process-global include state.
#[cfg(test)]
pub(crate) fn reset_global_eval_included_files() {}

/// Keeps crate unit tests isolated without process-global dynamic function state.
#[cfg(test)]
pub(crate) fn reset_global_eval_function_contexts() {}

/// Keeps crate unit tests isolated without process-global SPL autoload state.
#[cfg(test)]
pub(crate) fn reset_global_eval_autoload_contexts() {}

/// Keeps crate unit tests independent from process-global dynamic function cleanup.
#[cfg(test)]
pub(crate) fn unregister_global_eval_functions_for_context(_context: *mut ElephcEvalContext) {}

/// Lets test-local contexts register callbacks without introducing process-global ownership.
#[cfg(test)]
pub(crate) fn register_global_eval_autoload_context(
    _context: *mut ElephcEvalContext,
    _prepend: bool,
) -> bool {
    true
}

/// Keeps test callback unregistration local to the owning context.
#[cfg(test)]
pub(crate) fn unregister_global_eval_autoload_context(_context: *mut ElephcEvalContext) {}

/// Returns the process-local map from `define()`d constant name to its owning eval context.
#[cfg(not(test))]
fn global_eval_constants() -> &'static Mutex<HashMap<String, usize>> {
    GLOBAL_EVAL_CONSTANTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Records that `context` defined the constant `key` (already normalized).
///
/// PHP constants are request-global, but each eval context keeps its own table: a `define()` run
/// by a file an autoloader included -- inside the autoloader's owner context -- was invisible to
/// the context that then ran the class's methods. symfony/polyfill-intl-grapheme's `Grapheme.php`
/// opens with exactly such a `define('SYMFONY_GRAPHEME_CLUSTER_RX', ...)`, and every console
/// table died on "undefined constant" once its width computation reached the polyfill.
///
/// Answers false when another context already owns the name, which is php's "already defined".
#[cfg(not(test))]
pub(super) fn register_global_eval_constant(key: &str, context: *mut ElephcEvalContext) -> bool {
    if global_eval_orphan_constants()
        .lock()
        .map_or(true, |orphans| orphans.contains_key(key))
    {
        return false;
    }
    let Ok(mut constants) = global_eval_constants().lock() else {
        return false;
    };
    match constants.get(key).copied() {
        Some(existing) => existing == context as usize,
        None => {
            constants.insert(key.to_string(), context as usize);
            true
        }
    }
}

/// Returns the constants that outlived the context which `define()`d them.
#[cfg(not(test))]
fn global_eval_orphan_constants() -> &'static Mutex<HashMap<String, usize>> {
    GLOBAL_EVAL_ORPHAN_CONSTANTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Answers a constant another context `define()`d (`key` already normalized).
///
/// The defining context is often short-lived: the include an autoloader performs from compiled
/// code runs in a context of its own, freed once the include returns, and the constant has to
/// stay defined for the rest of the request. [`adopt_dying_context_constants`] moves those values
/// here before the context goes.
#[cfg(not(test))]
pub(super) fn global_eval_constant(
    key: &str,
    asking: *const ElephcEvalContext,
) -> Option<RuntimeCellHandle> {
    let owner = global_eval_constants()
        .lock()
        .ok()
        .and_then(|constants| constants.get(key).copied());
    if let Some(owner) = owner {
        if owner == asking as usize {
            return None;
        }
        let owner = unsafe { (owner as *const ElephcEvalContext).as_ref() }?;
        return owner.constants.get(key).copied();
    }
    global_eval_orphan_constants()
        .lock()
        .ok()?
        .get(key)
        .map(|handle| RuntimeCellHandle::from_raw(*handle as *mut crate::value::RuntimeCell))
}

/// Keeps the constants a context defined alive past that context's destruction.
///
/// php constants belong to the request, not to whichever eval context ran the `define()`. The
/// values are not released when a context is dropped, so moving the handles is enough; the web
/// request reset forgets them together with the arena that holds them.
#[cfg(not(test))]
pub(crate) fn adopt_dying_context_constants(context: &mut ElephcEvalContext) {
    let dying = context as *mut ElephcEvalContext as usize;
    let owned: Vec<String> = match global_eval_constants().lock() {
        Ok(mut constants) => {
            let names = constants
                .iter()
                .filter(|(_, owner)| **owner == dying)
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>();
            for name in &names {
                constants.remove(name);
            }
            names
        }
        Err(_) => return,
    };
    let Ok(mut orphans) = global_eval_orphan_constants().lock() else {
        return;
    };
    for name in owned {
        if let Some(handle) = context.constants.remove(&name) {
            orphans.insert(name, handle.as_ptr() as usize);
        }
    }
}

/// Publishes a constant the COMPILED program defined (`define('NAME', ...)`) to interpreted code.
///
/// php constants are one request-wide namespace, but a compiled `define()` stores its value in a
/// program global the interpreter cannot read, so `defined('NAME')` and a bare `NAME` inside an
/// eval()ed fragment or an included Blade view answered "undefined". The compiled `define()` hands
/// its boxed value here instead; it lands in the same table a context's constants move to when the
/// context dies, which every lookup consults last. A repeated publication replaces the value, as
/// the next `--web` request's `define()` of the same name must.
///
/// `handle` is owned by the registry from here on (it lives in the request arena, which the
/// request boundary wipes together with the table: see [`reset_global_eval_constants`]).
#[cfg(not(test))]
pub(crate) fn publish_compiled_constant(name: &str, handle: RuntimeCellHandle) {
    let key = normalize_constant_name(name);
    if let Ok(mut orphans) = global_eval_orphan_constants().lock() {
        orphans.insert(key, handle.as_ptr() as usize);
    }
}

/// Forgets every request-defined constant at a `--web` request boundary.
///
/// Both tables hold addresses in the request arena, which the boundary wipes; a constant that
/// survived it would also make the next request's `define()` of the same name fail.
#[cfg(not(test))]
pub(crate) fn reset_global_eval_constants() {
    if let Ok(mut constants) = global_eval_constants().lock() {
        constants.clear();
    }
    if let Ok(mut orphans) = global_eval_orphan_constants().lock() {
        orphans.clear();
    }
}

/// Registers a dynamic PHP function name to the context that declared it.
#[cfg(not(test))]
pub(super) fn register_global_eval_function(
    name: &str,
    context: *mut ElephcEvalContext,
) -> bool {
    if context.is_null() {
        return false;
    }
    let key = normalize_global_function_name(name);
    if key.is_empty() {
        return false;
    }
    let Ok(mut functions) = global_eval_functions().lock() else {
        return false;
    };
    match functions.get(&key).copied() {
        Some(existing) => existing == context as usize,
        None => {
            functions.insert(key, context as usize);
            let Some(context) = (unsafe { context.as_ref() }) else {
                return false;
            };
            context.retain_global_function();
            true
        }
    }
}

/// Records that a context now owns at least one request-global SPL autoload callback.
#[cfg(not(test))]
pub(crate) fn register_global_eval_autoload_context(
    context: *mut ElephcEvalContext,
    prepend: bool,
) -> bool {
    if context.is_null() {
        return false;
    }
    let Ok(mut contexts) = global_eval_autoload_contexts().lock() else {
        return false;
    };
    if contexts.contains(&(context as usize)) {
        return true;
    }
    if prepend {
        contexts.insert(0, context as usize);
    } else {
        contexts.push(context as usize);
    }
    let Some(context) = (unsafe { context.as_ref() }) else {
        return false;
    };
    context.retain_autoload_context();
    true
}

/// Removes a context after its final SPL autoload callback was unregistered.
#[cfg(not(test))]
pub(crate) fn unregister_global_eval_autoload_context(context: *mut ElephcEvalContext) {
    if context.is_null() {
        return;
    }
    let removed = global_eval_autoload_contexts()
        .lock()
        .ok()
        .is_some_and(|mut contexts| {
            contexts
                .iter()
                .position(|owner| *owner == context as usize)
                .map(|index| contexts.remove(index))
                .is_some()
        });
    if removed {
        release_global_eval_autoload_context(context);
    }
}

/// Finds the live context that declared one dynamically included PHP function.
#[cfg(not(test))]
pub(crate) fn global_eval_function_owner_context(
    name: &str,
) -> Option<*mut ElephcEvalContext> {
    let key = normalize_global_function_name(name);
    let functions = global_eval_functions().lock().ok()?;
    functions
        .get(&key)
        // PHP resolves an unqualified call made in a namespace against that namespace first,
        // then retries the global function table. Codegen supplies the resolved namespace form,
        // so reproduce the second probe for dynamic declarations discovered at runtime.
        .or_else(|| key.rsplit_once('\\').and_then(|(_, bare)| functions.get(bare)))
        .copied()
        .map(|context| context as *mut ElephcEvalContext)
}

/// Whether some live context declared exactly this function name.
///
/// `function_exists()` asks about ONE name: unlike a call, it has no namespace-to-global fallback,
/// so `function_exists('Ns\\f')` is false while only the global `f` exists.
#[cfg(not(test))]
pub(crate) fn global_eval_function_is_declared(name: &str) -> bool {
    let key = normalize_global_function_name(name);
    global_eval_functions()
        .lock()
        .map(|functions| functions.contains_key(&key))
        .unwrap_or(false)
}

/// Clones one dynamically included PHP function out of the context that declared it.
///
/// The owner entry is removed when its context is destroyed
/// (`unregister_global_eval_functions_for_context`), so a hit names a context that is still alive.
#[cfg(not(test))]
pub(crate) fn global_eval_function_declaration(
    name: &str,
) -> Option<crate::eval_ir::EvalFunction> {
    let owner = global_eval_function_owner_context(name)?;
    let owner = unsafe { owner.as_ref() }?;
    let bare = name.rsplit_once('\\').map_or(name, |(_, bare)| bare);
    owner
        .function(name)
        .or_else(|| owner.function(bare))
        .cloned()
}

/// Removes every function-owner entry associated with a context being destroyed.
#[cfg(not(test))]
pub(crate) fn unregister_global_eval_functions_for_context(context: *mut ElephcEvalContext) {
    if context.is_null() {
        return;
    }
    let context = context as usize;
    if let Ok(mut functions) = global_eval_functions().lock() {
        functions.retain(|_, owner| *owner != context);
    }
    // Constants are moved out before this runs (`adopt_dying_context_constants`); anything
    // still naming this context is a stale owner entry.
    if let Ok(mut constants) = global_eval_constants().lock() {
        constants.retain(|_, owner| *owner != context);
    }
}

/// Releases retained callback cells and then drops one request-global autoload ownership root.
#[cfg(not(test))]
fn release_global_eval_autoload_context(context: *mut ElephcEvalContext) {
    let Some(context_ref) = (unsafe { context.as_mut() }) else {
        return;
    };
    let callbacks = context_ref.take_autoload_callbacks();
    let mut values = crate::runtime_hooks::ElephcRuntimeOps::with_context(context.cast_const());
    for callback in callbacks {
        let _ = crate::interpreter::RuntimeValueOps::release(&mut values, callback);
    }
    if context_ref.forget_autoload_context() {
        unsafe { crate::ffi::context::finalize_eval_context_free(context) };
    }
}

/// Canonicalizes PHP function names for the process-global case-insensitive registry.
#[cfg(not(test))]
fn normalize_global_function_name(name: &str) -> String {
    name.trim_start_matches('\\').to_ascii_lowercase()
}

/// Records one eval-declared class so later eval contexts can see PHP-global metadata.
#[cfg(not(test))]
pub(super) fn register_global_eval_class(class: &Arc<EvalClass>) {
    let key = normalize_class_name(class.name());
    if let Ok(mut registry) = global_eval_classes().lock() {
        if !registry.classes.contains_key(&key) {
            registry.declared_class_names.push(class.name().to_string());
        }
        // Publishes the SAME allocation the declaring context keeps, so declaring a class costs
        // one `Arc` bump rather than a second deep copy of every method body.
        registry.classes.insert(key, Arc::clone(class));
    }
}

/// Records the physical declaration file for a process-global eval class.
#[cfg(not(test))]
pub(super) fn register_global_eval_class_source(class_name: &str, file: String) {
    if file.is_empty() {
        return;
    }
    let key = normalize_class_name(class_name);
    if let Ok(mut registry) = global_eval_classes().lock() {
        registry.class_source_files.insert(key, file);
    }
}

/// Records one eval-declared interface so later eval contexts can see PHP-global metadata.
#[cfg(not(test))]
pub(super) fn register_global_eval_interface(interface: &Arc<EvalInterface>) {
    let key = normalize_class_name(interface.name());
    if let Ok(mut registry) = global_eval_classes().lock() {
        if !registry.interfaces.contains_key(&key) {
            registry
                .declared_interface_names
                .push(interface.name().to_string());
        }
        registry.interfaces.insert(key, Arc::clone(interface));
    }
}

/// Records one eval-declared trait so later eval contexts can see PHP-global metadata.
#[cfg(not(test))]
pub(super) fn register_global_eval_trait(trait_decl: &Arc<EvalTrait>) {
    let key = normalize_class_name(trait_decl.name());
    if let Ok(mut registry) = global_eval_classes().lock() {
        if !registry.traits.contains_key(&key) {
            registry
                .declared_trait_names
                .push(trait_decl.name().to_string());
        }
        registry.traits.insert(key, Arc::clone(trait_decl));
    }
}

/// Records one eval-declared enum so later eval contexts can see PHP-global metadata.
#[cfg(not(test))]
pub(super) fn register_global_eval_enum(enum_decl: &Arc<EvalEnum>) {
    let key = normalize_class_name(enum_decl.name());
    if let Ok(mut registry) = global_eval_classes().lock() {
        if !registry.enums.contains_key(&key) {
            registry
                .declared_enum_names
                .push(enum_decl.name().trim_start_matches('\\').to_string());
        }
        registry.enums.insert(key, Arc::clone(enum_decl));
    }
}

/// Publishes one eval enum case singleton for every other context, keeping any published
/// backing value.
///
/// The cells live in the request arena like every other eval value, and the registry is
/// emptied at the same request boundary (`reset_global_eval_classes`), so a published address
/// never outlives its storage.
#[cfg(not(test))]
pub(super) fn register_global_eval_enum_case(
    enum_key: String,
    case_key: String,
    case: RuntimeCellHandle,
) {
    if let Ok(mut registry) = global_eval_classes().lock() {
        registry
            .enum_cases
            .entry((enum_key, case_key))
            .or_insert((0, None))
            .0 = case.as_ptr() as usize;
    }
}

/// Publishes one eval enum case's backing value, keeping any published singleton.
#[cfg(not(test))]
pub(super) fn register_global_eval_enum_case_value(
    enum_key: String,
    case_key: String,
    value: RuntimeCellHandle,
) {
    if let Ok(mut registry) = global_eval_classes().lock() {
        registry
            .enum_cases
            .entry((enum_key, case_key))
            .or_insert((0, None))
            .1 = Some(value.as_ptr() as usize);
    }
}

/// Returns an eval enum case singleton another context published, and its backing value.
#[cfg(not(test))]
pub(super) fn global_eval_enum_case(
    enum_key: &str,
    case_key: &str,
) -> Option<(RuntimeCellHandle, Option<RuntimeCellHandle>)> {
    let registry = global_eval_classes().lock().ok()?;
    let (case, value) = *registry
        .enum_cases
        .get(&(enum_key.to_string(), case_key.to_string()))?;
    if case == 0 {
        return None;
    }
    Some((
        RuntimeCellHandle::from_raw(case as *mut RuntimeCell),
        value.map(|value| RuntimeCellHandle::from_raw(value as *mut RuntimeCell)),
    ))
}

/// Records one eval-defined class-like alias for later generated eval contexts.
#[cfg(not(test))]
pub(super) fn register_global_eval_alias(alias_name: &str, alias: &EvalClassAlias) {
    let key = normalize_class_name(alias_name);
    if let Ok(mut registry) = global_eval_classes().lock() {
        registry.aliases.insert(key, alias.clone());
    }
}

/// Returns whether a class-like name was declared at runtime by any eval context.
///
/// Used to tell a callable that names a DYNAMIC class (`['Svc', 'make']`, `'Svc::make'`) from one
/// that names a compiled class or nothing at all: only the former needs the interpreter, and only
/// the former may be routed to the process-wide context that mirrors this registry.
#[cfg(not(test))]
pub(crate) fn global_eval_class_is_declared(name: &str) -> bool {
    let key = normalize_class_name(name);
    global_eval_classes()
        .lock()
        .is_ok_and(|registry| registry.classes.contains_key(&key))
}
