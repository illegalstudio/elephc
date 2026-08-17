//! Purpose:
//! The eval half of PHP's `ext/curl` callback options: libcurl calls a C trampoline
//! mid-transfer, that trampoline calls the adapter address installed in its slot, and this
//! module's adapter re-enters the eval INTERPRETER to run the PHP callable.
//!
//! Called from:
//! - `crate::interpreter::builtins::curl::handle`'s `eval_curl_setopt_apply` (option
//!   KIND 8), to install/clear a slot.
//! - `crate::interpreter::builtins::curl::{curl_exec, curl_multi_exec}`, to open and close
//!   the invocation frame around a transfer and to re-raise a parked throw afterwards.
//! - `crates/elephc-curl`'s trampolines, through [`eval_curl_callback_adapter`]'s address.
//!
//! # Why this is possible at all, when an earlier revision argued it was not
//!
//! `crate::interpreter::builtins::curl`'s module doc used to state that callbacks needed
//! "eval-interpreter callable invocation from C, which does not exist", because AOT installs
//! a callback by handing the bridge a codegen-emitted adapter address
//! (`src/codegen/runtime_callable_invoker.rs`) and "a pure Rust interpreter with no
//! generated assembly has no address to hand libcurl". That was simply wrong: an ordinary
//! `extern "C" fn` in THIS crate is an address, with the same C ABI, and the bridge stores
//! the adapter as an opaque `*const c_void` it calls through — it never asks where the
//! address came from (`crates/elephc-curl/src/callbacks.rs`'s `CallbackAdapter`). The AOT
//! adapter is generated only because compiled PHP's calling convention is generated; an
//! interpreter's is not.
//!
//! # The one real problem, and how it is solved: reaching the interpreter from C
//!
//! The adapter is `extern "C"` and receives only the three words the bridge stored. It must
//! reach `&mut ElephcEvalContext` and `&mut impl RuntimeValueOps`, both of which are
//! ordinary Rust borrows sitting on the stack of the `curl_exec()`/`curl_multi_exec()` frame
//! that is currently blocked inside libcurl.
//!
//! [`EvalCurlCallbackFrame`] is that bridge: [`eval_curl_with_callback_frame`] takes raw
//! pointers to both, publishes them, runs the transfer, and restores whatever was published
//! before. Soundness rests on four facts, none of them incidental:
//!
//! 1. **The borrows are genuinely idle across the call.** The raw pointers are derived
//!    immediately before the FFI call and neither original reference is touched until after
//!    it returns, so re-deriving `&mut` from them inside the adapter does not alias a live
//!    reborrow. This is the ordinary "pass `&mut` through a C callback's userdata" pattern.
//! 2. **`RuntimeValueOps` is a generic parameter, so the frame carries a MONOMORPHIZED
//!    THUNK** ([`EvalCurlCallbackFrame::invoke`]) alongside the erased `*mut c_void`. The
//!    adapter itself stays non-generic — there is exactly one address for the bridge to
//!    hold — while the thunk restores the concrete type. Storing a `&mut dyn
//!    RuntimeValueOps` instead would need the trait to be object-safe, which it is not
//!    obliged to stay.
//! 3. **FRAMES NEST, AND ONLY THE INNERMOST IS EVER READ.** PHP legitimately calls back into
//!    curl from inside a curl callback — `curl_pause($h, CURLPAUSE_CONT)` from a write
//!    callback is the documented way to resume a paused transfer, and `curl_exec($other)`
//!    from inside a callback is a shape `crates/elephc-curl/src/callbacks.rs` explicitly
//!    drops its table lock to permit. [`ActiveFrameGuard`] therefore SAVES the previously
//!    published frame, publishes the nested one, and restores the saved pointer on drop
//!    (RAII, so an early return or a panic restores too).
//!
//!    The aliasing argument survives nesting precisely because the adapter always reads
//!    [`ACTIVE_FRAME`], i.e. the INNERMOST frame: the outer frame's pointers are not touched
//!    while an inner transfer runs, so at no instant do two live `&mut` to one
//!    `ElephcEvalContext` exist. What actually happens is the ordinary nested-reborrow
//!    chain — outer raw pointer -> inner `&mut` (the inner builtin's own parameter) -> inner
//!    raw pointer — with every use at the innermost link. The outer frame's pointers stay
//!    valid throughout because the stack frame that owns them is still live, blocked inside
//!    libcurl, one level down the call stack.
//! 4. **Single-threaded.** `elephc-curl` documents the caller contract that no two
//!    `elephc_curl_*` calls for one id may run concurrently, and compiled PHP is effectively
//!    single-threaded; the frame slot is a process-global, matching the bridge's own
//!    `CALLBACK_THREW`.
//!
//! ## What is NOT the reentrancy hazard here
//!
//! An earlier revision of this comment claimed the interpreter contains no `RefCell` at all,
//! and rested "re-entry cannot trip a borrow-flag panic" on that. **The claim was false**:
//! `crate::context`'s `NATIVE_FRAME_CALLED_CLASS_OVERRIDES` is a
//! `thread_local! { RefCell<Vec<..>> }`. The conclusion still holds, but for the correct
//! reason — BORROW LIFETIME, not absence. Every one of its five borrows lives entirely
//! inside a single `.with(|overrides| ...)` closure in `crate::context::global_registry` and
//! is released before that closure returns; none is held across a call that could reach
//! libcurl, so a re-entering callback always finds the cell unborrowed. Anything added later
//! that holds a `borrow_mut()` across an FFI call would break this, which is exactly why the
//! basis is worth stating precisely.
//!
//! # No unwinding through libcurl, ever
//!
//! eval reports a PHP throw as an ordinary `Err(EvalStatus)` return value — there is no
//! `longjmp` and no Rust unwind to contain, so this side of the invariant is structural
//! rather than defended. The throw is PARKED on the frame, the adapter answers `status = -1`,
//! and the bridge turns that into its own per-callback abort code and raises the SAME
//! process-wide gate the AOT path uses. After the transfer returns,
//! [`eval_curl_resume_callback_throw`] consumes that gate (the bridge's
//! `elephc_curl_take_callback_threw`, exactly what `__rt_curl_rethrow_pending` consumes in
//! AOT) and resumes the parked status, so the throwable surfaces as an ordinary catchable
//! PHP exception AFTER `curl_exec()` — with `curl_errno()` answering `0`, which the bridge
//! guarantees by clearing the handle's error state on the `callback_threw` path.
//!
//! The gate ALSO suppresses every further callback for the rest of the transfer, on every
//! handle — php-src's own shape (`zend_call_function` refuses to call anything while
//! `EG(exception)` is set), and what makes a throw from one easy handle's callback during
//! `curl_multi_exec()` not get swallowed by a `try`/`catch` inside another handle's.
//!
//! ## Throws and nesting
//!
//! THE PARKED THROW TRAVELS WITH ITS FRAME, never through a shared slot: it is returned
//! alongside the transfer's own result and handed straight to [`eval_curl_resume_callback_
//! throw`] by the same statement. That is what makes nesting behave the way real PHP 8.4.20
//! does, measured against it directly:
//!
//! - An inner callback that throws while an OUTER transfer is in flight resumes as an
//!   ordinary `Err` out of the INNER `curl_exec()`. If the outer callback catches it, the
//!   outer transfer simply continues (measured: `inner-caught:RuntimeException | outer-ok:true
//!   | outerErrno=0 | innerErrno=0`) — which happens for free here, because a caught `Err`
//!   never reaches the outer frame at all.
//! - If the outer callback does NOT catch it, the `Err` propagates out of the callable, the
//!   outer adapter parks it on the OUTER frame, and the outer transfer aborts and re-raises
//!   it (measured: `outer-threw:RuntimeException:inner boom uncaught | outerErrno=0 |
//!   innerErrno=0`). Both errnos stay `0`, because each level's abort was an exception
//!   rather than a `CURLcode`.
//!
//! The bridge's process-wide `CALLBACK_THREW` pairs up level by level for the same reason:
//! the inner transfer's own `begin_transfer()` opens a fresh scope, the inner
//! [`eval_curl_resume_callback_throw`] swaps the flag back down, and the outer level only
//! ever raises it again through its own adapter's `status = -1`.
//!
//! A single shared slot for the parked throw would have been a real bug under exactly this
//! nesting, which is why there is not one.

use std::ffi::c_void;

use crate::curl_ffi as ffi;
use crate::stream_resources::EvalCurlCallbackSlot;

use super::*;

/// One in-flight transfer's re-entry point back into the interpreter.
///
/// Holds erased pointers plus the monomorphized thunk that restores their types — see this
/// module's header for the soundness argument.
struct EvalCurlCallbackFrame {
    /// The `&mut ElephcEvalContext` the blocked `curl_exec()`/`curl_multi_exec()` frame owns.
    context: *mut ElephcEvalContext,
    /// The `&mut V` for that frame's concrete `V: RuntimeValueOps`, type-erased.
    values: *mut c_void,
    /// Restores `values`'s type and runs one callback. Monomorphized per `V` at the call
    /// site, which is what lets [`eval_curl_callback_adapter`] stay non-generic.
    invoke: unsafe fn(&mut EvalCurlCallbackFrame, i64, usize, &mut ffi::CallSpec) -> i64,
    /// The normalized callables for this transfer, keyed by `(easy table id, slot)`. A
    /// `None` callable is the DEBUG slot's deliberate silence (`EvalCurlCallbackSlot::
    /// Silent`); the adapter answers without entering the interpreter for it.
    ///
    /// A Vec rather than a map because a transfer has at most six entries per handle and a
    /// multi run rarely more than a handful of handles.
    installed: Vec<(i64, usize, Option<EvaluatedCallable>)>,
    /// The first PHP throw a callback produced, resumed after the transfer returns. Only the
    /// FIRST is kept: the bridge's gate suppresses every later callback, so a second one
    /// cannot normally arrive, and if it did, php-src would keep the first throwable too.
    parked: Option<EvalStatus>,
}

// SAFETY: never actually sent anywhere. The frame is confined to one thread's stack window
// between `eval_curl_with_callback_frame`'s push and pop; the `unsafe impl` exists only
// because the pointer fields make the compiler's auto-derive say otherwise, and the static
// below has to name the type. `elephc-curl`'s own `EasyEntry`/`MultiEntry` carry the
// identical impl for the identical reason.
unsafe impl Send for EvalCurlCallbackFrame {}

/// The INNERMOST active frame, or null outside any transfer.
///
/// A process-global rather than a `thread_local!` deliberately, matching the bridge's own
/// `CALLBACK_THREW`: `elephc-curl`'s caller contract is single-threaded (its module docs say
/// so in three places), and a `thread_local!` would imply a per-thread story this whole
/// surface does not have.
///
/// Written only through [`ActiveFrameGuard`], which makes the save/restore discipline
/// structural rather than a convention every call site has to remember.
static ACTIVE_FRAME: std::sync::atomic::AtomicPtr<EvalCurlCallbackFrame> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

/// Publishes a frame for its lifetime and restores the previously published one on drop.
///
/// RAII rather than a matched pair of stores because the restore has to survive every exit
/// path, including an early `?` and a panic unwinding out of the transfer. Leaving a dead
/// stack frame published would be a dangling `*mut ElephcEvalContext` the next callback
/// would dereference.
struct ActiveFrameGuard {
    /// Whatever was published when this guard was created — null at the outermost level,
    /// the enclosing transfer's frame when nested.
    previous: *mut EvalCurlCallbackFrame,
}

impl ActiveFrameGuard {
    /// Publishes `frame` and remembers its predecessor.
    ///
    /// # Safety
    /// `frame` must stay live and pinned until this guard is dropped.
    unsafe fn publish(frame: *mut EvalCurlCallbackFrame) -> Self {
        let previous = ACTIVE_FRAME.swap(frame, std::sync::atomic::Ordering::SeqCst);
        Self { previous }
    }
}

impl Drop for ActiveFrameGuard {
    fn drop(&mut self) {
        ACTIVE_FRAME.store(self.previous, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Maps a `CURLOPT_*FUNCTION` option number to its bridge slot index, or `None` for an
/// option that is not a callback at all.
///
/// The numbers are the AOT prelude's own literals (`crate::curl_prelude::curl_setopt`'s
/// `$kind === 8` branch), which are in turn `crates/elephc-curl/src/callbacks.rs`'s slot
/// assignment. `CURLOPT_PROGRESSFUNCTION` and `CURLOPT_XFERINFOFUNCTION` keep SEPARATE slots
/// because php-src keeps two separate handler records and libcurl gives xferinfo precedence
/// when both are registered; merging them would make "set progress last" win, which is not
/// what either does.
pub(in crate::interpreter) fn eval_curl_callback_slot(option: i64) -> Option<(usize, &'static str)> {
    match option {
        20_011 => Some((ffi::SLOT_WRITE as usize, "CURLOPT_WRITEFUNCTION")),
        20_079 => Some((ffi::SLOT_HEADER as usize, "CURLOPT_HEADERFUNCTION")),
        20_012 => Some((ffi::SLOT_READ as usize, "CURLOPT_READFUNCTION")),
        20_056 => Some((ffi::SLOT_PROGRESS as usize, "CURLOPT_PROGRESSFUNCTION")),
        20_094 => Some((ffi::SLOT_DEBUG as usize, "CURLOPT_DEBUGFUNCTION")),
        20_219 => Some((ffi::SLOT_XFERINFO as usize, "CURLOPT_XFERINFOFUNCTION")),
        _ => None,
    }
}

/// Applies `curl_setopt($ch, <a KIND 8 option>, $value)`.
///
/// Mirrors `crate::curl_prelude::curl_setopt`'s `$kind === 8` branch, minus its
/// `CURLOPT_INFILE` interplay (eval does not carry the PHP-stream options, so the read slot
/// holds the user's callable directly and its `$fd` argument is always `null` — which is
/// also exactly what php-src passes for a handle with no `CURLOPT_INFILE`).
pub(in crate::interpreter) fn eval_curl_setopt_callback(
    raw: i64,
    table_id: i64,
    slot: usize,
    option_name: &str,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if values.type_tag(value)? == EVAL_TAG_NULL {
        // THE DEBUG SLOT IS NEVER DEREGISTERED ONCE TOUCHED. Clearing the registration does
        // not restore "nothing", it restores libcurl's OWN default, which with
        // `CURLOPT_VERBOSE` on writes the whole trace to the process's fd 2 — php prints
        // nothing there, because its trampoline stays installed with no callable behind it.
        // `EvalCurlCallbackSlot::Silent` reproduces that; see its own doc and
        // `crate::curl_prelude::curl_setopt`'s matching `$slot === 4` branch.
        let cleared = if slot == ffi::SLOT_DEBUG as usize {
            EvalCurlCallbackSlot::Silent
        } else {
            EvalCurlCallbackSlot::Empty
        };
        context
            .stream_resources_mut()
            .set_curl_easy_callback(table_id, slot, cleared, values)?;
        if slot == ffi::SLOT_WRITE as usize {
            // php-src keeps ONE write mode and `CURLOPT_WRITEFUNCTION => null` restores
            // STDOUT — never whatever `CURLOPT_RETURNTRANSFER` was set to earlier (measured
            // on 8.4.20: `curl_exec()` then prints the body and returns `true`).
            context
                .stream_resources_mut()
                .set_curl_easy_write_mode(table_id, false, false);
        }
        return values.bool_value(eval_curl_apply_bridge_slot(raw, table_id, slot, cleared));
    }
    // php-src validates the callable EAGERLY, at `curl_setopt()` time (its own
    // `is_callable($value)` guard), and so does the AOT prelude — a bad callback is a
    // `TypeError` here, not a silent failure at transfer time.
    //
    // NORMALIZATION IS NOT VALIDATION, and conflating them was a real bug in an earlier
    // draft of this file: `eval_callable("no_such_function_at_all")` succeeds, producing a
    // `Named` callable for a name nothing defines, so `curl_setopt()` accepted it and the
    // failure only appeared as a silently skipped callback at transfer time. The probe is
    // `is_callable()`'s own.
    let callable_is_valid = match eval_callable(value, context, values) {
        Ok(callable) => eval_callable_probe_exists(&callable, context, values)?,
        Err(_) => false,
    };
    if !callable_is_valid {
        let message = if values.type_tag(value)? == EVAL_TAG_STRING {
            let name = values.string_bytes(value)?;
            let name = String::from_utf8_lossy(&name);
            format!(
                "curl_setopt(): Argument #3 ($value) must be a valid callback for option \
                 {option_name}, function \"{name}\" not found or invalid function name"
            )
        } else {
            format!(
                "curl_setopt(): Argument #3 ($value) must be a valid callback for option \
                 {option_name}, no array or string given"
            )
        };
        return eval_throw_type_error(&message, context, values);
    }
    let installed = EvalCurlCallbackSlot::Callable(value);
    if !eval_curl_apply_bridge_slot(raw, table_id, slot, installed) {
        return values.bool_value(false);
    }
    // ROOTED ONLY AFTER THE BRIDGE ACCEPTED IT, matching the AOT prelude's ordering — the
    // caller's `value` cell is still live until this returns, so nothing can be released
    // between the two steps.
    context
        .stream_resources_mut()
        .set_curl_easy_callback(table_id, slot, installed, values)?;
    if slot == ffi::SLOT_WRITE as usize {
        // Installing `CURLOPT_WRITEFUNCTION` selects php-src's `PHP_CURL_USER`, which
        // deselects `PHP_CURL_RETURN`: with a write callback installed last, `curl_exec()`
        // returns `true` even when `CURLOPT_RETURNTRANSFER` was set, and the body reaches
        // only the callback.
        context
            .stream_resources_mut()
            .set_curl_easy_write_mode(table_id, false, true);
    }
    values.bool_value(true)
}

/// Installs or clears one bridge slot for an eval handle.
///
/// THE TWO OPAQUE WORDS ARE SMALL INTEGERS, NOT POINTERS, and that is what makes the
/// registration safe to leave in place between transfers: `descriptor` is `slot + 1` (the
/// `+ 1` only so it is non-null, which is the bridge's "this slot holds a callable" test)
/// and `self_obj` is the handle's eval table key. Neither can dangle, so — unlike the AOT
/// side, whose `descriptor` is a real rooted callable pointer — nothing here has to be torn
/// down when a transfer ends. The bridge entry itself dies with the handle
/// (`elephc_curl_easy_free`), which this table drives at context teardown.
fn eval_curl_apply_bridge_slot(
    raw: i64,
    table_id: i64,
    slot: usize,
    state: EvalCurlCallbackSlot,
) -> bool {
    let Ok(slot_index) = i32::try_from(slot) else {
        return false;
    };
    let (descriptor, self_obj, adapter) = match state {
        EvalCurlCallbackSlot::Empty => (
            std::ptr::null_mut::<c_void>(),
            std::ptr::null_mut::<c_void>(),
            std::ptr::null::<c_void>(),
        ),
        EvalCurlCallbackSlot::Silent | EvalCurlCallbackSlot::Callable(_) => (
            (slot + 1) as *mut c_void,
            table_id as usize as *mut c_void,
            eval_curl_callback_adapter as *const c_void,
        ),
    };
    // SAFETY: the two opaque words are integers the bridge only ever hands back to the
    // adapter (see this function's doc), and `adapter` is this module's own `extern "C"`
    // function with exactly the signature `crate::curl_ffi::CallSpec` documents.
    unsafe { ffi::easy_set_callback(raw, slot_index, descriptor, self_obj, adapter) }
}

/// Re-installs a source handle's callback set onto a `curl_copy_handle()` duplicate.
///
/// CALLBACKS ARE RE-REGISTERED, NEVER INHERITED, exactly as in
/// `crate::curl_prelude::curl_copy_handle`: libcurl's own `dupset` copies the callback
/// function pointers AND their `CURLOPT_*DATA`, and every one of those data values is the
/// ORIGINAL handle's id — a duplicate left as libcurl made it would call back naming the
/// original's `$ch` and read the original's slots. The bridge clears every registration on
/// the duplicate for that reason; this puts them back pointing at the copy.
///
/// THE WRITE SLOT IS ROOTED BUT NOT NECESSARILY RE-REGISTERED, the AOT loop's own rule:
/// anything that later selected `PHP_CURL_RETURN` or `PHP_CURL_STDOUT` leaves the callable
/// rooted but INACTIVE, and re-registering it on the copy would re-select `PHP_CURL_USER` —
/// desyncing the two sides, because installing a write callback also clears
/// `return_transfer`. The decision is the ACTIVE-MODE mirror (`write_user`), which
/// `return_transfer` alone cannot express.
pub(in crate::interpreter) fn eval_curl_copy_callbacks(
    source_id: i64,
    copy_id: i64,
    copy_raw: i64,
    copy_write_user: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for (slot, state) in context.stream_resources().curl_easy_callbacks(source_id) {
        let register = slot != ffi::SLOT_WRITE as usize || copy_write_user;
        if register {
            eval_curl_apply_bridge_slot(copy_raw, copy_id, slot, state);
        }
        context
            .stream_resources_mut()
            .set_curl_easy_callback(copy_id, slot, state, values)?;
    }
    Ok(())
}

/// Clears every callback slot on a handle, on both sides — `curl_reset()`'s counterpart to
/// `crates/elephc-curl/src/callbacks.rs`'s `clear_all` (which the bridge's own
/// `elephc_curl_easy_reset` already ran; this drops the eval-side roots to match).
pub(in crate::interpreter) fn eval_curl_clear_callbacks(
    table_id: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    context
        .stream_resources_mut()
        .clear_curl_easy_callbacks(table_id, values)
}

/// Runs `transfer` with an active callback frame covering `handles`, so any callback libcurl
/// fires during it can re-enter the interpreter.
///
/// `handles` is every eval easy-handle table key the transfer may drive: one for
/// `curl_exec()`, every attached handle for `curl_multi_exec()`. Handles with no installed
/// callback contribute nothing and cost nothing.
///
/// THE RAW POINTERS ARE TAKEN LAST, immediately before `transfer` runs, and neither `context`
/// nor `values` is touched again until it returns — see this module's header for why that
/// ordering is the soundness argument and not a stylistic preference.
pub(in crate::interpreter) fn eval_curl_with_callback_frame<V, R>(
    handles: &[i64],
    context: &mut ElephcEvalContext,
    values: &mut V,
    transfer: impl FnOnce() -> R,
) -> Result<(R, Option<EvalStatus>), EvalStatus>
where
    V: RuntimeValueOps,
{
    let mut installed = Vec::new();
    for &table_id in handles {
        for (slot, state) in context.stream_resources().curl_easy_callbacks(table_id) {
            let callable = match state {
                EvalCurlCallbackSlot::Callable(cell) => {
                    // Normalized ONCE per transfer rather than once per callback
                    // invocation: a write callback fires per chunk, and re-resolving a
                    // method/closure callable thousands of times would be pure overhead.
                    Some(eval_callable(cell, context, values)?)
                }
                EvalCurlCallbackSlot::Silent => None,
                EvalCurlCallbackSlot::Empty => continue,
            };
            installed.push((table_id, slot, callable));
        }
    }
    let mut frame = EvalCurlCallbackFrame {
        context: context as *mut ElephcEvalContext,
        values: (values as *mut V).cast::<c_void>(),
        invoke: eval_curl_invoke_typed::<V>,
        installed,
        parked: None,
    };
    {
        // SAFETY: `frame` outlives the guard — it is this function's own local and the guard
        // is dropped at the end of this block, before `frame` is moved from or goes out of
        // scope. A nested transfer started from inside `transfer` publishes its own frame
        // and restores THIS one when it finishes; see this module's header for why only the
        // innermost frame is ever read and why that keeps the aliasing argument intact.
        let _published = unsafe { ActiveFrameGuard::publish(&raw mut frame) };
        let outcome = transfer();
        drop(_published);
        // THE PARKED THROW IS RETURNED, NOT STASHED. Handing it back with the transfer's own
        // result is what keeps nesting correct: a shared slot would let an inner transfer's
        // throw be picked up by an outer level (or clobber one already waiting there). The
        // caller resumes it in the very next statement — it still has to consult the
        // bridge's gate to know a throw is why the transfer stopped, which is why resuming
        // is a separate call rather than an `Err` from here.
        Ok((outcome, frame.parked.take()))
    }
}

/// Resumes a callback's parked throw after the transfer that produced it has returned.
///
/// GATED ON THE BRIDGE'S OWN FLAG as well as on `parked`, which is the same discipline
/// `crates/elephc-curl/src/callbacks.rs`'s `CALLBACK_THREW` doc argues for on the AOT side:
/// the bridge is what knows a callback threw DURING THIS TRANSFER. Reading the flag
/// unconditionally is also what CLEARS it, so a throw that somehow outlived its transfer
/// cannot wedge the next one.
pub(in crate::interpreter) fn eval_curl_resume_callback_throw(
    parked: Option<EvalStatus>,
) -> Result<(), EvalStatus> {
    let threw = ffi::take_callback_threw();
    match (threw, parked) {
        (true, Some(status)) => Err(status),
        // The bridge saw a throw but nothing was parked: unreachable through this module's
        // own adapter (it parks before it reports), kept as a loud-but-safe fallback rather
        // than an assumption, so the transfer cannot silently report success after a
        // callback aborted it.
        (true, None) => Err(EvalStatus::RuntimeFatal),
        // A parked throw with the gate already down means an INNER transfer consumed the
        // gate for the same throwable — impossible today (an inner throw travels back as an
        // ordinary `Err` and never reaches an outer frame's `parked`), so resuming it is the
        // conservative answer: losing a throwable silently would be worse than raising one
        // twice.
        (false, Some(status)) => Err(status),
        (false, None) => Ok(()),
    }
}

/// The address the bridge stores in a slot and calls through. NON-GENERIC on purpose: there
/// is exactly one of it, and the frame's own thunk restores the `RuntimeValueOps` type.
///
/// `descriptor` is `slot + 1` and `self_obj` is the easy handle's eval table key — the two
/// opaque words [`eval_curl_apply_bridge_slot`] installed.
///
/// # Safety
/// Called by `crates/elephc-curl`'s trampolines with a `spec` valid for the duration of the
/// call, honouring `crate::curl_ffi::CallSpec`'s layout.
unsafe extern "C" fn eval_curl_callback_adapter(
    descriptor: *mut c_void,
    self_obj: *mut c_void,
    spec: *mut ffi::CallSpec,
) -> i64 {
    let frame = ACTIVE_FRAME.load(std::sync::atomic::Ordering::SeqCst);
    if frame.is_null() || spec.is_null() {
        // No transfer of ours is running, so there is no interpreter to re-enter. Not
        // reachable in practice — an eval curl handle cannot escape its `eval()` and only
        // this module ever drives one — but answering "did nothing" is the only safe reply
        // available here, and it is the same reply the bridge's own trampolines give for an
        // empty slot.
        return 0;
    }
    // SAFETY: `frame` is the live stack frame `eval_curl_with_callback_frame` published a
    // moment ago and clears before returning; the transfer that reached this adapter is
    // still running inside that function.
    let frame = unsafe { &mut *frame };
    let spec = unsafe { &mut *spec };
    let table_id = self_obj as usize as i64;
    let slot = (descriptor as usize).wrapping_sub(1);
    // SAFETY: `invoke` is the thunk monomorphized for the same `V` whose pointer the frame
    // carries.
    unsafe { (frame.invoke)(frame, table_id, slot, spec) }
}

/// The monomorphized half of the adapter: restores `V`, runs the PHP callable, and writes
/// the answer back into `spec`.
///
/// # Safety
/// `frame.context`/`frame.values` must be the live, currently-idle borrows the transfer's
/// own stack frame owns, and `V` must be the type `frame.values` was erased from.
unsafe fn eval_curl_invoke_typed<V: RuntimeValueOps>(
    frame: &mut EvalCurlCallbackFrame,
    table_id: i64,
    slot: usize,
    spec: &mut ffi::CallSpec,
) -> i64 {
    let Some(index) = frame
        .installed
        .iter()
        .position(|(id, index, _)| *id == table_id && *index == slot)
    else {
        return 0;
    };
    // `EvalCurlCallbackSlot::Silent`: registered so libcurl's own default cannot take over,
    // with nothing behind it. php-src's answer for the same state, and never an error.
    if frame.installed[index].2.is_none() {
        return 0;
    }
    // SAFETY: see this function's contract; both pointers were derived from live `&mut`s
    // that the blocked transfer frame is not using. Neither borrows `frame` itself — the
    // field reads end here — which is what lets the callable be borrowed OUT of `frame`
    // below and `frame.parked` be written afterwards.
    let context = unsafe { &mut *frame.context };
    let values = unsafe { &mut *frame.values.cast::<V>() };
    // The callable is BORROWED rather than cloned: `EvaluatedCallable` is not `Clone`, and
    // a write callback fires once per received chunk, so cloning it would be both
    // impossible and wasteful. The borrow ends with this statement, before `frame.parked`
    // is written.
    let outcome = eval_curl_run_callback(
        frame.installed[index]
            .2
            .as_ref()
            .expect("callable presence checked immediately above"),
        table_id,
        spec,
        context,
        values,
    );
    match outcome {
        Ok(result) => result,
        Err(status) => {
            // PARKED, NOT PROPAGATED: there is no Rust frame between here and libcurl that
            // could carry an `Err`, so the throw waits for `eval_curl_rethrow_pending_
            // callback_throw` and the bridge is told to abort through `status`.
            if frame.parked.is_none() {
                frame.parked = Some(status);
            }
            spec.status = -1;
            0
        }
    }
}

/// Marshals one callback's arguments, invokes the PHP callable, and encodes its answer the
/// way `spec.result_kind` asks for.
fn eval_curl_run_callback<V: RuntimeValueOps>(
    callable: &EvaluatedCallable,
    table_id: i64,
    spec: &mut ffi::CallSpec,
    context: &mut ElephcEvalContext,
    values: &mut V,
) -> Result<i64, EvalStatus> {
    // ARGUMENT 0 IS ALWAYS `$ch`, supplied here rather than by the bridge: only the
    // interpreter knows the eval table key behind the handle. Re-boxing the key is safe for
    // the same reason `curl_multi_info_read()` re-boxes one — an eval curl cell owns
    // nothing.
    //
    // THESE PER-INVOCATION CELLS ARE NOT RELEASED AFTER THE CALL, and that is a known,
    // bounded cost rather than an oversight — but it IS a cost, so it is written down.
    // `bind_method_scope_args` binds every argument into the callee's scope as
    // `ScopeCellOwnership::Borrowed`, and `ElephcEvalScope::drain_owned_cells` frees only
    // `Owned` entries, so the callback's own frame teardown does not reclaim them; nor does
    // any other callback-taking builtin in this interpreter reclaim the cells it builds
    // (`preg_replace_callback`'s per-match `$matches` array, `array_map`, `array_filter`,
    // `usort`). They are reclaimed with the eval context's heap.
    //
    // The bound is "cells per invocation x invocations in one eval context": 2 for
    // write/header, 3 for read and debug, 5 for progress/xferinfo. `CURLOPT_WRITEFUNCTION`
    // is the one that matters, because it fires once per received chunk. Releasing them
    // here would diverge from that interpreter-wide convention in exactly the direction
    // that turns a bounded leak into a potential use-after-free if any path does take
    // ownership, so the convention is followed and the cost is documented instead — in
    // `docs/php/curl.md`'s eval section as well as here.
    let mut args = vec![values.curl_handle(table_id)?];
    let argc = usize::try_from(spec.argc).unwrap_or(0);
    for index in 0..argc {
        // SAFETY: the bridge guarantees `argv` points at `argc` `CallArg`s for the duration
        // of the call (`crates/elephc-curl/src/callbacks.rs`'s trampolines build them on
        // their own stacks).
        let arg = unsafe { &*spec.argv.add(index) };
        let cell = match arg.tag {
            ffi::CALL_TAG_INT => values.int(arg.lo)?,
            ffi::CALL_TAG_NULL => values.null()?,
            ffi::CALL_TAG_STRING => {
                let len = usize::try_from(arg.hi).unwrap_or(0);
                let ptr = arg.lo as usize as *const u8;
                if len == 0 || ptr.is_null() {
                    values.string_bytes_value(&[])?
                } else {
                    // COPIED IMMEDIATELY: these bytes are libcurl's own transient receive
                    // buffer, valid only for this call.
                    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
                    values.string_bytes_value(bytes)?
                }
            }
            // Defensively, any tag a future bridge might add: the three named above are the
            // only ones the six trampolines produce today, and PHP `null` is the least
            // surprising stand-in for one this build does not know.
            _ => values.null()?,
        };
        args.push(cell);
    }
    let result = eval_evaluated_callable_with_values(callable, args, context, values)?;
    debug_assert!(
        spec.result_kind == ffi::RESULT_INT || spec.result_kind == ffi::RESULT_STRING,
        "the bridge only ever asks for an int or a string result"
    );
    if spec.result_kind == ffi::RESULT_STRING {
        // php-src reads the read callback's answer as a STRING and treats anything else as
        // end-of-data — `Z_TYPE(retval) == IS_STRING` in `curl_read`, not a cast — and
        // truncates a longer string with `MIN(size * nmemb, len)` rather than failing
        // (measured against 8.4.20).
        if values.type_tag(result)? != EVAL_TAG_STRING {
            spec.out_len = 0;
            return Ok(0);
        }
        let bytes = values.string_bytes(result)?;
        let capacity = usize::try_from(spec.out_cap).unwrap_or(0);
        let written = bytes.len().min(capacity);
        if written > 0 && !spec.out_buf.is_null() {
            // SAFETY: the bridge passes libcurl's own upload buffer, writable for `out_cap`
            // bytes, and `written` is clamped to it.
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), spec.out_buf, written);
            }
        }
        spec.out_len = written as i64;
        return Ok(0);
    }
    eval_int_value(result, values)
}
