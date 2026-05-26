//! Purpose:
//! Emits runtime support for PHP 8.1-style cooperative Fibers.
//! Owns object layout constants, stack allocation, context switching, and public API helper wiring.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()`.
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
//! | 168    | 8    | user_arg_max      | how many start_args slots `start()` may write — leaves trailing slots untouched so `new Fiber(use(...))` captures survive |
//! | 176    | 56   | float_args[0..7]  | parallel slot file for float captures (loaded into d0..d6 by the trampoline) |
//!
//! Total payload = 232 bytes.

mod alloc;
mod api;
mod entry;
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
/// Byte offset of the `state` field (0=NotStarted 1=Running 2=Suspended 3=Terminated).
pub(crate) const FIBER_STATE_OFFSET: i32 = 8;
/// Byte offset of the `stack_base` field (low address of the fiber stack region, includes guard page).
pub(crate) const FIBER_STACK_BASE_OFFSET: i32 = 16;
/// Byte offset of the `stack_top` field (high address; initial SP for a fresh fiber, 16-aligned).
pub(crate) const FIBER_STACK_TOP_OFFSET: i32 = 24;
/// Byte offset of the `stack_size` field (total bytes of the mmap'd stack region, required by munmap).
pub(crate) const FIBER_STACK_SIZE_OFFSET: i32 = 32;
/// Byte offset of the `saved_sp` field (SP saved when this fiber is not running; entry point for first switch).
pub(crate) const FIBER_SAVED_SP_OFFSET: i32 = 40;
/// Byte offset of the `callable` field (closure/function pointer supplied to the Fiber constructor).
pub(crate) const FIBER_CALLABLE_OFFSET: i32 = 48;
/// Byte offset of the `callable_wrapper` field (generated ABI adapter that runs the Fiber body on first switch).
pub(crate) const FIBER_CALLABLE_WRAPPER_OFFSET: i32 = 56;
/// Byte offset of the `caller` field (Fiber* of the resumer, NULL when the fiber has not been started).
pub(crate) const FIBER_CALLER_OFFSET: i32 = 64;
/// Byte offset of the low word of `transfer_value` (Mixed cell holding the value in transit between fibers).
pub(crate) const FIBER_TRANSFER_VALUE_OFFSET: i32 = 72;
/// Byte offset of the `pending_throw` field (Throwable* scheduled by Fiber::throw; cleared before re-raising).
pub(crate) const FIBER_PENDING_THROW_OFFSET: i32 = 88;
/// Byte offset of `own_exc_head` (saved _exc_handler_top for this fiber's exception handler chain).
pub(crate) const FIBER_OWN_EXC_HEAD_OFFSET: i32 = 96;
/// Byte offset of `own_call_frame` (saved _exc_call_frame_top for this fiber's cleanup chain).
pub(crate) const FIBER_OWN_CALL_FRAME_OFFSET: i32 = 104;
/// Byte offset of the first `start_args` slot (up to 7 Mixed pointers passed to Fiber::start; parallels AArch64 int arg regs).
pub(crate) const FIBER_START_ARGS_OFFSET: i32 = 112;
/// Maximum number of `start_args` slots (one per AArch64 integer argument register minus `$this`).
pub(crate) const FIBER_START_ARGS_MAX: i32 = 7;
/// Byte offset of `user_arg_max` (controls how many start_args slots start() may write; trailing slots survive for `new Fiber(use(...))` captures).
pub(crate) const FIBER_USER_ARG_MAX_OFFSET: i32 = 168;
/// Byte offset of the first `float_args` slot (parallel slot file for float captures; loaded into d0..d6 by the trampoline).
pub(crate) const FIBER_FLOAT_ARGS_OFFSET: i32 = 176;
/// Maximum number of `float_args` slots.
pub(crate) const FIBER_FLOAT_ARGS_MAX: i32 = 7;
/// Total size of the Fiber object payload in bytes (heap-allocated; class_id at offset 0, followed by all runtime-managed fields).
pub(crate) const FIBER_OBJECT_SIZE: i32 = 232;

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
