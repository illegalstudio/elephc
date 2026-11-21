//! Purpose:
//! Emits runtime support for PHP 8.1-style cooperative Fibers.
//! Owns object layout constants, stack allocation, context switching, and public API helper wiring.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - Fiber state, saved registers, stack ownership, and transfer values must stay consistent across suspend/resume/throw paths.
//!
//! Object layout (allocated via __rt_heap_alloc, "object instance" kind):
//!
//! | Offset | Size | Field             | Notes                                 |
//! |--------|------|-------------------|---------------------------------------|
//! | -8     | 8    | heap kind = 4     | written by allocator                  |
//! | 0      | 8    | class_id          | runtime class id of `Fiber`           |
//! | 8      | 8    | state             | 0=NotStarted 1=Running 2=Suspended 3=Terminated |
//! | 16     | 8    | stack_base        | low address of fiber stack (heap ptr) |
//! | 24     | 8    | stack_top         | high address (initial SP, 16-aligned) |
//! | 32     | 8    | stack_size        | total bytes of the stack region       |
//! | 40     | 8    | saved_sp          | SP saved when fiber is not running    |
//! | 48     | 8    | callable          | callable descriptor pointer           |
//! | 56     | 8    | callable_wrapper  | generated Fiber entry ABI adapter     |
//! | 64     | 8    | caller            | Fiber* of resumer (NULL = main)       |
//! | 72     | 16   | transfer_value    | mixed cell — value in transit         |
//! | 88     | 8    | pending_throw     | Throwable* to rethrow on resume       |
//! | 96     | 8    | own_exc_head      | saved _exc_handler_top for this fiber |
//! | 104    | 8    | own_call_frame    | saved _exc_call_frame_top for this fiber |
//! | 112    | 56   | start_args[0..7]  | up to 7 Mixed pointers passed to start() (one per AArch64 int arg-reg minus $this) |
//! | 168    | 8    | user_arg_max      | how many start_args slots `start()` may write |
//! | 176    | 8    | start_arg_count   | number of boxed values copied by `start()` |
//! | 184    | 48   | reserved          | legacy slot file; descriptor captures now carry hidden Fiber callback args |
//! | 232    | 8    | own_magic_set_guard | saved `__set` reentrancy chain head for this fiber |
//! | 240    | 8    | own_gc_release_suppressed | saved deep-release GC protection for this fiber |
//!
//! Total payload = 248 bytes.

mod alloc;
mod api;
mod entry;
pub(crate) mod force_close;
mod switch;

pub(crate) use alloc::{emit_fiber_alloc_stack, emit_fiber_free_stack};
pub(crate) use api::{
    emit_fiber_construct, emit_fiber_get_current, emit_fiber_get_return, emit_fiber_resume,
    emit_fiber_start, emit_fiber_state_getter, emit_fiber_suspend, emit_fiber_throw,
    emit_fiber_throw_state_error,
};
pub(crate) use entry::emit_fiber_entry;
pub(crate) use switch::emit_fiber_switch;

// ── Fiber object field offsets ───────────────────────────────────────
pub(crate) use elephc_memoir::coroutine_storage::{
    FIBER_STATE_OFFSET,
    FIBER_STACK_BASE_OFFSET,
    FIBER_STACK_TOP_OFFSET,
    FIBER_STACK_SIZE_OFFSET,
    FIBER_SAVED_SP_OFFSET,
    FIBER_CALLABLE_OFFSET,
    FIBER_CALLABLE_WRAPPER_OFFSET,
    FIBER_CALLER_OFFSET,
    FIBER_TRANSFER_VALUE_OFFSET,
    FIBER_PENDING_THROW_OFFSET,
    FIBER_OWN_CALL_FRAME_OFFSET,
    FIBER_START_ARGS_OFFSET,
    FIBER_START_ARGS_MAX,
    FIBER_USER_ARG_MAX_OFFSET,
    FIBER_START_ARG_COUNT_OFFSET,
    FIBER_OBJECT_SIZE,
};

// ── Lifecycle states (stored in FIBER_STATE_OFFSET) ──────────────────
// Phase 3 (suspend) will introduce the first user of FIBER_STATE_SUSPENDED.
/// Fiber has not been started; start() has not been called yet.
pub(crate) const FIBER_STATE_NOT_STARTED: i32 = 0;
/// Fiber is currently executing on its own stack (entered via start/resume/throw).
pub(crate) const FIBER_STATE_RUNNING: i32 = 1;
#[allow(dead_code)]
/// Fiber is paused at a Fiber::suspend() call; can be resumed.
pub(crate) const FIBER_STATE_SUSPENDED: i32 = 2;
/// Fiber's callable has returned or an exception escaped uncaught; getReturn() is valid.
pub(crate) const FIBER_STATE_TERMINATED: i32 = 3;

// ── Default per-fiber stack size ─────────────────────────────────────
/// Default usable stack size in bytes for a newly constructed Fiber (256 KiB; excludes the guard page).
pub(crate) const FIBER_DEFAULT_STACK_SIZE: i32 = 256 * 1024;
