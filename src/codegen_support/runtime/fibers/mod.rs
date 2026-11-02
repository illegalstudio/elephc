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
//!
//! Total payload = 240 bytes.

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

/// How many freed fiber stacks `_fiber_stack_cache` keeps for reuse.
///
/// Every generator runs on a fiber, and a fresh stack is an `mmap` plus an `mprotect` for its
/// guard page, returned with a `munmap`: three syscalls per generator. A Symfony page renders
/// each Twig template and block as a generator, and those syscalls reached 5% of a request.
/// A stack freed with its guard page still installed can be handed out again as-is, so the
/// cache holds a few; nesting deeper than this only falls back to the kernel.
pub(crate) const FIBER_STACK_CACHE_SLOTS: usize = 16;

/// How many handed-out fiber stacks `_fiber_live_stacks` tracks.
///
/// A generator still referenced when a `--web` request ends is dropped with the heap arena, and
/// nothing freed its stack: every such generator leaked a 2 MB mapping and made the next request
/// map a fresh one. Symfony's Twig page left eight per request -- 16 MB of address space, and
/// eight mappings towards the kernel's limit, each time. The request boundary now hands every
/// stack still tracked here back to the cache. A stack handed out while the table is full is not
/// tracked and keeps the old behavior.
pub(crate) const FIBER_LIVE_STACK_SLOTS: usize = 1024;

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
/// Byte offset of the saved runtime-name `__set` reentrancy chain for this fiber.
pub(crate) const FIBER_OWN_MAGIC_SET_GUARD_OFFSET: i32 = 232;
/// Byte offset of the first `start_args` slot (up to 7 Mixed pointers passed to Fiber::start; parallels AArch64 int arg regs).
pub(crate) const FIBER_START_ARGS_OFFSET: i32 = 112;
/// Maximum number of `start_args` slots (one per AArch64 integer argument register minus `$this`).
pub(crate) const FIBER_START_ARGS_MAX: i32 = 7;
/// Byte offset of `user_arg_max` (controls how many start_args slots start() may write).
pub(crate) const FIBER_USER_ARG_MAX_OFFSET: i32 = 168;
/// Byte offset storing how many start_args slots were supplied by the most recent start() call.
pub(crate) const FIBER_START_ARG_COUNT_OFFSET: i32 = 176;
/// Byte offset of the first reserved legacy float slot.
pub(crate) const FIBER_FLOAT_ARGS_OFFSET: i32 = 184;
/// Maximum number of reserved legacy float slots.
pub(crate) const FIBER_FLOAT_ARGS_MAX: i32 = 6;
/// Byte offset of the trailing start-argument slots only GENERATORS use, past every Fiber field.
///
/// A generator boxes each parameter, closure captures included, into a start slot until its body
/// first runs. Seven were too few: Laravel's `Query\Builder::orderedLazyById()` returns a
/// generator closure with ten, and the whole query builder fell back to the interpreter.
/// `Fiber::start()` keeps its seven, since they mirror the argument registers it forwards.
pub(crate) const GENERATOR_EXTRA_ARGS_OFFSET: i32 = 240;
/// Number of trailing generator-only start-argument slots.
pub(crate) const GENERATOR_EXTRA_ARGS_MAX: i32 = 9;
/// Total start-argument slots a generator may fill: the Fiber ones, then the trailing ones.
pub(crate) const GENERATOR_START_ARGS_MAX: i32 = FIBER_START_ARGS_MAX + GENERATOR_EXTRA_ARGS_MAX;
/// Total size of the Fiber object payload in bytes (heap-allocated; class_id at offset 0, followed by all runtime-managed fields).
pub(crate) const FIBER_OBJECT_SIZE: i32 = GENERATOR_EXTRA_ARGS_OFFSET + GENERATOR_EXTRA_ARGS_MAX * 8;

/// Byte offset of generator start-argument slot `index`: the Fiber slots first, then the
/// trailing generator-only ones.
pub(crate) fn generator_start_arg_offset(index: usize) -> usize {
    let fiber_slots = FIBER_START_ARGS_MAX as usize;
    if index < fiber_slots {
        FIBER_START_ARGS_OFFSET as usize + index * 8
    } else {
        GENERATOR_EXTRA_ARGS_OFFSET as usize + (index - fiber_slots) * 8
    }
}

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
/// Default usable stack size in bytes for a newly constructed Fiber.
///
/// PHP uses a 2 MiB C stack on 64-bit targets (`4096 * 512`); matching that
/// baseline also leaves room for bridge calls made from generator bodies.
/// The guard page is allocated separately.
pub(crate) const FIBER_DEFAULT_STACK_SIZE: i32 = 2 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    /// Locks the 64-bit PHP-compatible default used by both Fiber and Generator stacks.
    #[test]
    fn default_stack_matches_the_php_64_bit_baseline() {
        assert_eq!(FIBER_DEFAULT_STACK_SIZE, 4096 * 512);
    }
}
