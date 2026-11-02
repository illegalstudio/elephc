//! Purpose:
//! Holds the request's `static` state for eval-declared code: static property values, their
//! reference aliases, and the scope every `static` local lives in.
//!
//! Called from:
//! - The static accessors on `ElephcEvalContext` (`runtime_state.rs`) and the `--web` request
//!   reset.
//!
//! Key details:
//! - php statics belong to the request, not to an eval context. Each compiled function activation
//!   that reaches the bridge builds a context of its own, so per-context storage made a static
//!   written through one activation invisible to the next: `static $n; $n++` counted from zero
//!   again and `self::$instances` emptied every time the calling function returned.
//! - Thread-local rather than a mutex-guarded static: the handles are raw pointers (not `Send`),
//!   the interpreter runs on one thread per process (web workers are forked), and each test
//!   thread gets a store of its own.
//! - Handles are never released here. A `--web` boundary forgets them with the arena that holds
//!   them; a CLI process keeps them until exit, as php does.

use super::*;

/// The request-scoped `static` state shared by every eval context on this thread.
struct RequestStatics {
    scope: Box<ElephcEvalScope>,
    properties: HashMap<(String, String), RuntimeCellHandle>,
    aliases: HashMap<(String, String), EvalReferenceTarget>,
    /// `strtok()`'s tokenizer state: the string and the byte offset of the next scan. Plain
    /// bytes, no handles, so a reset simply drops it.
    strtok: Option<(Vec<u8>, usize)>,
}

impl RequestStatics {
    /// Builds an empty store.
    fn new() -> Self {
        Self {
            scope: Box::new(ElephcEvalScope::new()),
            properties: HashMap::new(),
            aliases: HashMap::new(),
            strtok: None,
        }
    }
}

thread_local! {
    static REQUEST_STATICS: RefCell<RequestStatics> = RefCell::new(RequestStatics::new());
}

/// Returns the scope every `static` local slot lives in.
///
/// A raw pointer for the reason the per-context scope was one: writes arrive through code that
/// only holds a shared context. The box is replaced only by [`reset_request_statics`], at a
/// request boundary where no activation is left to hold the pointer.
pub(super) fn request_static_scope_ptr() -> *mut ElephcEvalScope {
    REQUEST_STATICS.with(|statics| {
        let mut statics = statics.borrow_mut();
        std::ptr::from_mut::<ElephcEvalScope>(&mut *statics.scope)
    })
}

/// Returns a stored static property cell.
pub(super) fn request_static_property(key: &(String, String)) -> Option<RuntimeCellHandle> {
    REQUEST_STATICS.with(|statics| statics.borrow().properties.get(key).copied())
}

/// Stores a static property cell, returning the one it replaced.
pub(super) fn set_request_static_property(
    key: (String, String),
    cell: RuntimeCellHandle,
) -> Option<RuntimeCellHandle> {
    REQUEST_STATICS.with(|statics| statics.borrow_mut().properties.insert(key, cell))
}

/// Removes a stored static property cell, handing it back to the caller to release.
pub(super) fn take_request_static_property(key: &(String, String)) -> Option<RuntimeCellHandle> {
    REQUEST_STATICS.with(|statics| statics.borrow_mut().properties.remove(key))
}

/// Returns the reference target a static property is bound to.
pub(super) fn request_static_property_alias(
    key: &(String, String),
) -> Option<EvalReferenceTarget> {
    REQUEST_STATICS.with(|statics| statics.borrow().aliases.get(key).cloned())
}

/// Binds a static property to a reference target, returning the previous binding.
pub(super) fn bind_request_static_property_alias(
    key: (String, String),
    target: EvalReferenceTarget,
) -> Option<EvalReferenceTarget> {
    REQUEST_STATICS.with(|statics| statics.borrow_mut().aliases.insert(key, target))
}

/// Runs `f` on the request's `strtok()` state (`None` until a string was given).
pub(crate) fn with_request_strtok<R>(f: impl FnOnce(&mut Option<(Vec<u8>, usize)>) -> R) -> R {
    REQUEST_STATICS.with(|statics| f(&mut statics.borrow_mut().strtok))
}

/// Forgets every request static at a `--web` boundary.
///
/// The handles point into the arena the boundary wipes, so forgetting (not releasing) is the
/// right operation, exactly as `forget_request_scoped_state` does for a retained context.
pub(crate) fn reset_request_statics() {
    REQUEST_STATICS.with(|statics| *statics.borrow_mut() = RequestStatics::new());
}
