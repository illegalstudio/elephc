//! One inclusion-state owner shared by all eval contexts in a PHP request.
//! Contexts retain handles, not copied file sets, so reset reaches surviving contexts.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Native index access. Callbacks and returned aligned, writable u64 cells must
/// remain valid for the program lifetime. Native writes and bridge access must be
/// serialized by the PHP execution owner. The path-to-cell mapping is immutable;
/// neither callback may execute PHP/reenter. Paths presented here are canonical keys.
#[derive(Clone, Copy)]
pub(crate) struct NativeIncludeHooks {
    pub lookup: unsafe extern "C" fn(*const u8, u64) -> *mut u64,
    pub reset: unsafe extern "C" fn(),
}

impl NativeIncludeHooks {
    unsafe fn cell(self, path: &Path) -> *mut u64 {
        let bytes = path.as_os_str().as_encoded_bytes();
        unsafe { (self.lookup)(bytes.as_ptr(), bytes.len() as u64) }
    }
}

#[derive(Default)]
pub(super) struct RequestIncludeState {
    included: HashSet<PathBuf>,
    native: Option<NativeIncludeHooks>,
}

/// Guard-cell value the COMPILER's `__rt_source_include_prime` writes for a source it already
/// included, as opposed to the `1` a runtime include writes. Mirrored by `COMPILER_INCLUDED` in
/// `src/codegen/source_units/bridge.rs` — the two constants are one ABI and must agree.
const COMPILER_INCLUDED: u64 = 2;

impl RequestIncludeState {
    pub(super) fn contains(&self, path: &Path) -> bool {
        if let Some(hooks) = self.native {
            // The unsafe registration contract keeps callbacks/cells live and access serialized.
            let cell = unsafe { hooks.cell(path) };
            if !cell.is_null() { return unsafe { cell.read() != 0 }; }
        }
        self.included.contains(path)
    }

    /// Whether the COMPILER performed this inclusion, rather than this request.
    ///
    /// Only the native table can answer: the shadow set exists solely for sources the compiled
    /// program never saw, and by definition the compiler included none of those.
    pub(super) fn compiler_included(&self, path: &Path) -> bool {
        let Some(hooks) = self.native else { return false };
        let cell = unsafe { hooks.cell(path) };
        !cell.is_null() && unsafe { cell.read() } == COMPILER_INCLUDED
    }

    pub(super) fn mark(&mut self, path: PathBuf) {
        if let Some(hooks) = self.native {
            let cell = unsafe { hooks.cell(&path) };
            if !cell.is_null() {
                // A runtime inclusion must never DOWNGRADE the compiler's own mark: the file
                // stays one the compiler included for the rest of the request, and lowering the
                // cell to 1 here would make the next include of it re-run a body the program
                // already executed at startup.
                if unsafe { cell.read() } == 0 { unsafe { cell.write(1); } }
                return;
            }
        }
        self.included.insert(path);
    }

    pub(super) fn reset(&mut self) {
        self.included.clear();
        if let Some(hooks) = self.native { unsafe { (hooks.reset)(); } }
    }

    /// Binds native storage without keeping a shadow inclusion bit.
    ///
    /// # Safety
    /// The caller must uphold NativeIncludeHooks' lifetime, alignment, serialization
    /// and non-reentrancy contract for all later operations on this request state.
    pub(super) unsafe fn install_native(&mut self, hooks: NativeIncludeHooks) -> bool {
        if let Some(current) = self.native {
            return std::ptr::fn_addr_eq(current.lookup, hooks.lookup)
                && std::ptr::fn_addr_eq(current.reset, hooks.reset);
        }
        // Transfer only sources already opened dynamically; registration itself
        // must never mark merely compiled/discovered sources as included -- and, like `mark`,
        // must never downgrade a compiler-included cell the prime already raised.
        self.included.retain(|path| {
            let cell = unsafe { hooks.cell(path) };
            if cell.is_null() {
                true
            } else {
                if unsafe { cell.read() } == 0 { unsafe { cell.write(1); } }
                false
            }
        });
        self.native = Some(hooks);
        true
    }
}

pub(super) type SharedIncludeState = Arc<Mutex<RequestIncludeState>>;

/// Installs process-lifetime native storage access for a context or the current request.
///
/// # Safety
/// The NativeIncludeHooks contract must hold for every later request-state operation.
pub(crate) unsafe fn install_native_include_hooks(
    context: Option<&crate::context::ElephcEvalContext>,
    hooks: NativeIncludeHooks,
) -> bool {
    let state = context.map(|context| context.include_state.clone()).unwrap_or_else(current_include_state);
    let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    unsafe { state.install_native(hooks) }
}

/// What the current request's inclusion state says about one canonical path, read without a
/// context: `(included, compiler_included)`. Every generated-code context shares this state, so
/// the answer is the one any of them would give.
pub(crate) fn request_include_marks(path: &Path) -> (bool, bool) {
    let state = current_include_state();
    let state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    (state.contains(path), state.compiler_included(path))
}

/// Generated-code contexts belong to the current worker request. Standalone unit
/// contexts default to isolated requests; sharing is explicit in lifecycle tests.
pub(super) fn current_include_state() -> SharedIncludeState {
    #[cfg(not(test))]
    {
        static STATE: std::sync::OnceLock<SharedIncludeState> = std::sync::OnceLock::new();
        STATE.get_or_init(SharedIncludeState::default).clone()
    }
    #[cfg(test)]
    SharedIncludeState::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ElephcEvalContext;

    thread_local! { static NATIVE_FLAG: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }

    unsafe extern "C" fn lookup(path: *const u8, length: u64) -> *mut u64 {
        let bytes = unsafe { std::slice::from_raw_parts(path, length as usize) };
        if bytes == b"/native.php" { NATIVE_FLAG.with(std::cell::Cell::as_ptr) }
        else { std::ptr::null_mut() }
    }

    unsafe extern "C" fn reset_native() { NATIVE_FLAG.with(|flag| flag.set(0)); }

    #[test]
    fn request_include_state_uses_native_cells_without_a_shadow_copy() {
        let mut state = RequestIncludeState::default();
        unsafe { reset_native(); assert!(state.install_native(NativeIncludeHooks { lookup, reset: reset_native })); }
        assert!(!state.contains(Path::new("/native.php")));
        NATIVE_FLAG.with(|flag| flag.set(1));
        assert!(state.contains(Path::new("/native.php")));
        state.reset();
        assert!(!state.contains(Path::new("/native.php")));
        state.mark("/native.php".into());
        NATIVE_FLAG.with(|flag| assert_eq!(flag.get(), 1));
        NATIVE_FLAG.with(|flag| flag.set(0));
        assert!(!state.contains(Path::new("/native.php")));
        state.mark("/dynamic.php".into());
        assert!(state.contains(Path::new("/dynamic.php")));
        state.reset();
        assert!(!state.contains(Path::new("/dynamic.php")));
    }

    /// The compiler's mark and a runtime inclusion are both "included", but only one of them
    /// means the file's body already ran at program start.
    #[test]
    fn compiler_included_is_distinguishable_from_a_runtime_inclusion() {
        let mut state = RequestIncludeState::default();
        unsafe { reset_native(); assert!(state.install_native(NativeIncludeHooks { lookup, reset: reset_native })); }
        let native = Path::new("/native.php");
        assert!(!state.contains(native));
        assert!(!state.compiler_included(native));

        state.mark(native.to_path_buf());
        assert!(state.contains(native), "a runtime inclusion still counts as included");
        assert!(!state.compiler_included(native), "a runtime inclusion is not a compile-time one");

        // What `__rt_source_include_prime` writes.
        NATIVE_FLAG.with(|flag| flag.set(COMPILER_INCLUDED));
        assert!(state.contains(native));
        assert!(state.compiler_included(native));

        // A later runtime `mark` of the same file must not lower the cell back to 1.
        state.mark(native.to_path_buf());
        assert!(state.compiler_included(native), "mark downgraded the compiler's own inclusion");

        // A source the compiled program never saw has no cell, so it can never be compiler-included.
        state.mark("/dynamic.php".into());
        assert!(state.contains(Path::new("/dynamic.php")));
        assert!(!state.compiler_included(Path::new("/dynamic.php")));
        state.reset();
    }

    #[test]
    fn request_include_state_transfers_only_previously_opened_sources() {
        let mut state = RequestIncludeState::default();
        state.mark("/native.php".into());
        state.mark("/dynamic.php".into());
        unsafe { reset_native(); assert!(state.install_native(NativeIncludeHooks { lookup, reset: reset_native })); }
        assert_eq!(state.included.len(), 1);
        assert!(state.contains(Path::new("/native.php")));
        assert!(state.contains(Path::new("/dynamic.php")));
        assert!(!state.contains(Path::new("/never-opened.php")));
        state.reset();
    }

    fn context(state: &SharedIncludeState) -> ElephcEvalContext {
        let mut context = ElephcEvalContext::new();
        context.include_state = state.clone();
        context
    }

    #[test]
    fn request_include_state_is_shared_and_reset_reaches_surviving_contexts() {
        let state = SharedIncludeState::default();
        let mut first = context(&state);
        let second = context(&state);
        first.mark_included_file("/included.php");
        assert!(second.has_included_file("/included.php"));
        state.lock().unwrap().reset();
        assert!(!first.has_included_file("/included.php"));
        assert!(!second.has_included_file("/included.php"));
    }

    #[test]
    fn request_include_state_outlives_a_context_without_crossing_requests() {
        let state = SharedIncludeState::default();
        let mut first = context(&state);
        first.mark_included_file("/included.php");
        drop(first);
        assert!(context(&state).has_included_file("/included.php"));
        assert!(!context(&SharedIncludeState::default()).has_included_file("/included.php"));
    }
}
