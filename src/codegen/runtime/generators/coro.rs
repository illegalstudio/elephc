//! Purpose:
//! Fiber-backed generator coroutine runtime. A PHP `Generator` is a coroutine
//! object that reuses the Fiber 232-byte layout (so it can reuse
//! `__rt_fiber_switch`/`suspend`/`resume`/`throw`/`start`) plus a small block of
//! generator-specific fields in the otherwise-unused reserved region. This file
//! owns those field offsets and the `yield` suspension primitive
//! `__rt_gen_suspend`.
//!
//! Called from:
//! - `crate::codegen::runtime::generators::emit_generator_runtime()`.
//! - Generated generator bodies call `__rt_gen_suspend` at each `yield`.
//!
//! Key details:
//! - `__rt_gen_suspend` records the yielded key/value into the generator's
//!   persistent `last_key`/`last_value` slots (so repeated `current()`/`key()`
//!   reads are pure loads), then reuses `__rt_fiber_suspend`. Because the fiber
//!   suspend boundary re-raises a scheduled `pending_throw` *inside* the
//!   coroutine's own stack, `Generator::throw()` lands in an in-generator
//!   `try/catch` — the core of issue #329.
//! - Generator fields live at offsets 184..224, inside the Fiber `reserved`
//!   region that `__rt_fiber_construct` already zero-initialises.

// Some field offsets are consumed only by codegen wiring landed in later
// commits of the generators-on-fibers migration; keep them defined alongside
// the layout they describe.
#![allow(dead_code)]

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::runtime::fibers::{
    FIBER_STATE_NOT_STARTED, FIBER_STATE_OFFSET, FIBER_STATE_SUSPENDED, FIBER_STATE_TERMINATED,
};

/// Runtime Mixed tag for an integer payload (used to box auto-increment keys).
const INT_TAG: i64 = 0;

// ── Generator-specific field offsets (within the reused Fiber object) ──────
/// Byte offset of `gen_last_key`: boxed Mixed of the most recent yield key.
pub(crate) const GEN_LAST_KEY_OFFSET: i32 = 184;
/// Byte offset of `gen_last_value`: boxed Mixed of the most recent yield value.
pub(crate) const GEN_LAST_VALUE_OFFSET: i32 = 192;
/// Byte offset of `gen_return_value`: boxed Mixed of the body `return` value.
pub(crate) const GEN_RETURN_VALUE_OFFSET: i32 = 200;
/// Byte offset of `gen_auto_key`: next auto-increment integer key (raw i64).
pub(crate) const GEN_AUTO_KEY_OFFSET: i32 = 208;
/// Byte offset of `gen_delegated_iter`: inner iterator for `yield from`.
pub(crate) const GEN_DELEGATED_ITER_OFFSET: i32 = 216;

/// Emits `__rt_gen_suspend`, the `yield` suspension primitive shared by every
/// generated generator body.
///
/// Records the yielded key/value into the current generator's persistent slots
/// (refcount-replacing the previous occupants), then suspends via
/// `__rt_fiber_suspend`. On resume it returns the value delivered by the next
/// `send()`/`next()` (owned by the caller), or — when `Generator::throw()`
/// scheduled a `pending_throw` — the fiber suspend boundary re-raises that
/// exception inside this generator's stack so a local `try/catch` can handle it.
///
/// Input:  `x0`/`rdi` = boxed key cell (NULL → auto-increment integer key);
///         `x1`/`rsi` = boxed value cell (ownership moves into the generator).
/// Output: `x0`/`rax` = boxed value delivered by the next resume (owned).
pub(crate) fn emit_gen_suspend(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_gen_suspend_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_suspend ---");
    emitter.label_global("__rt_gen_suspend");

    // -- prologue: park the boxed key/value and cache the generator object --
    emitter.instruction("sub sp, sp, #48");                                     // reserve frame plus saved callee registers
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("stp x19, x20, [sp, #16]");                             // preserve callee-saved x19/x20 used as caches
    emitter.instruction("str x21, [sp]");                                       // preserve callee-saved x21 used for the parked key
    emitter.instruction("add x29, sp, #32");                                    // anchor the new frame pointer
    emitter.instruction("mov x20, x1");                                         // x20 = boxed yielded value (ownership moving into the generator)
    emitter.instruction("mov x21, x0");                                         // x21 = boxed key cell (NULL means auto-increment)
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "x19", "_fiber_current", 0); // x19 = the generator coroutine currently running

    // -- record the yielded value into gen_last_value (refcount-replace) --
    emitter.instruction(&format!("ldr x9, [x19, #{}]", GEN_LAST_VALUE_OFFSET)); // x9 = previous last_value occupant
    emitter.instruction(&format!("str x20, [x19, #{}]", GEN_LAST_VALUE_OFFSET)); // store the freshly yielded value
    emitter.instruction("mov x0, x9");                                          // pass the previous occupant to the releaser
    emitter.instruction("bl __rt_decref_mixed");                                // release the previous last_value (NULL is safe)

    // -- record the key: explicit cell, or a boxed auto-increment integer --
    emitter.instruction("cbz x21, __rt_gen_suspend_auto_key");                  // branch to the auto-key path when no explicit key was supplied
    emitter.instruction(&format!("ldr x9, [x19, #{}]", GEN_LAST_KEY_OFFSET));   // x9 = previous last_key occupant
    emitter.instruction(&format!("str x21, [x19, #{}]", GEN_LAST_KEY_OFFSET));  // store the explicit yielded key
    emitter.instruction("mov x0, x9");                                          // pass the previous key occupant to the releaser
    emitter.instruction("bl __rt_decref_mixed");                                // release the previous last_key (NULL is safe)
    emitter.instruction("b __rt_gen_suspend_yield");                            // skip the auto-key path

    emitter.label("__rt_gen_suspend_auto_key");
    emitter.instruction(&format!("ldr x1, [x19, #{}]", GEN_AUTO_KEY_OFFSET));   // x1 = current auto-increment integer key payload
    emitter.instruction(&format!("mov x0, #{}", INT_TAG));                      // runtime tag 0 = integer
    emitter.instruction("mov x2, #0");                                          // integer payload has no high word
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = boxed integer key cell (owned)
    emitter.instruction(&format!("ldr x9, [x19, #{}]", GEN_LAST_KEY_OFFSET));   // x9 = previous last_key occupant
    emitter.instruction(&format!("str x0, [x19, #{}]", GEN_LAST_KEY_OFFSET));   // store the freshly boxed auto key
    emitter.instruction("mov x0, x9");                                          // pass the previous key occupant to the releaser
    emitter.instruction("bl __rt_decref_mixed");                                // release the previous last_key (NULL is safe)
    emitter.instruction(&format!("ldr x9, [x19, #{}]", GEN_AUTO_KEY_OFFSET));   // x9 = current auto-increment counter
    emitter.instruction("add x9, x9, #1");                                      // advance the auto-increment counter
    emitter.instruction(&format!("str x9, [x19, #{}]", GEN_AUTO_KEY_OFFSET));   // persist the advanced counter for the next bare yield

    // -- suspend back to the resumer; reuse the fiber suspend boundary --
    emitter.label("__rt_gen_suspend_yield");
    emitter.instruction("mov x0, #0");                                          // generators read last_value, so the fiber transfer value is unused
    emitter.instruction("bl __rt_fiber_suspend");                               // suspend; on resume re-raises a pending throw or returns the sent value

    // -- epilogue: x0 already holds the resumer-delivered value (owned) --
    emitter.instruction("ldr x21, [sp]");                                       // restore caller's x21
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore caller's x19/x20
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the sent value to the generator body
}

/// x86_64 implementation of `__rt_gen_suspend`.
///
/// Mirrors the ARM64 version using the System V ABI: generator object cached in
/// `r12`, parked key/value in `r13`/`r14`, Mixed boxing via `rax`=tag,
/// `rdi`=low, `rsi`=high.
fn emit_gen_suspend_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_suspend ---");
    emitter.label_global("__rt_gen_suspend");

    // -- prologue: park the boxed key/value and cache the generator object --
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("push r12");                                            // preserve r12 (generator object cache)
    emitter.instruction("push r13");                                            // preserve r13 (parked boxed value)
    emitter.instruction("push r14");                                            // preserve r14 (parked boxed key)
    emitter.instruction("sub rsp, 8");                                          // keep the stack 16-byte aligned across nested calls
    emitter.instruction("mov r14, rdi");                                        // r14 = boxed key cell (NULL means auto-increment)
    emitter.instruction("mov r13, rsi");                                        // r13 = boxed yielded value (ownership moving into the generator)
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "r12", "_fiber_current", 0); // r12 = the generator coroutine currently running

    // -- record the yielded value into gen_last_value (refcount-replace) --
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", GEN_LAST_VALUE_OFFSET)); // rax = previous last_value occupant
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], r13", GEN_LAST_VALUE_OFFSET)); // store the freshly yielded value
    emitter.instruction("call __rt_decref_mixed");                              // release the previous last_value (NULL is safe)

    // -- record the key: explicit cell, or a boxed auto-increment integer --
    emitter.instruction("test r14, r14");                                       // was an explicit key supplied?
    emitter.instruction("jz __rt_gen_suspend_auto_key");                        // branch to the auto-key path when no explicit key was supplied
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", GEN_LAST_KEY_OFFSET)); // rax = previous last_key occupant
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], r14", GEN_LAST_KEY_OFFSET)); // store the explicit yielded key
    emitter.instruction("call __rt_decref_mixed");                              // release the previous last_key (NULL is safe)
    emitter.instruction("jmp __rt_gen_suspend_yield");                          // skip the auto-key path

    emitter.label("__rt_gen_suspend_auto_key");
    emitter.instruction(&format!("mov rdi, QWORD PTR [r12 + {}]", GEN_AUTO_KEY_OFFSET)); // rdi = current auto-increment integer key payload
    emitter.instruction(&format!("mov rax, {}", INT_TAG));                      // runtime tag 0 = integer
    emitter.instruction("xor esi, esi");                                        // integer payload has no high word
    emitter.instruction("call __rt_mixed_from_value");                          // rax = boxed integer key cell (owned)
    emitter.instruction(&format!("mov rcx, QWORD PTR [r12 + {}]", GEN_LAST_KEY_OFFSET)); // rcx = previous last_key occupant
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], rax", GEN_LAST_KEY_OFFSET)); // store the freshly boxed auto key
    emitter.instruction("mov rax, rcx");                                        // move the previous key occupant into the decref argument register
    emitter.instruction("call __rt_decref_mixed");                              // release the previous last_key (NULL is safe)
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", GEN_AUTO_KEY_OFFSET)); // rax = current auto-increment counter
    emitter.instruction("add rax, 1");                                          // advance the auto-increment counter
    emitter.instruction(&format!("mov QWORD PTR [r12 + {}], rax", GEN_AUTO_KEY_OFFSET)); // persist the advanced counter for the next bare yield

    // -- suspend back to the resumer; reuse the fiber suspend boundary --
    emitter.label("__rt_gen_suspend_yield");
    emitter.instruction("xor edi, edi");                                        // generators read last_value, so the fiber transfer value is unused
    emitter.instruction("call __rt_fiber_suspend");                             // suspend; on resume re-raises a pending throw or returns the sent value

    // -- epilogue: rax already holds the resumer-delivered value (owned) --
    emitter.instruction("add rsp, 8");                                          // release the alignment pad
    emitter.instruction("pop r14");                                             // restore caller's r14
    emitter.instruction("pop r13");                                             // restore caller's r13
    emitter.instruction("pop r12");                                             // restore caller's r12
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the sent value to the generator body
}

/// Emits the fiber-backed `__rt_gen_*` accessor helpers: `current`, `key`,
/// `valid`, `next`, `send`, `throw`, `rewind`, `getReturn`.
///
/// Each accessor drives the underlying coroutine through the fiber primitives:
/// it lazily starts the generator on first access (`__rt_fiber_start`),
/// advances it (`__rt_fiber_resume`/`__rt_fiber_throw`), and reads the
/// persistent `gen_last_value`/`gen_last_key`/`gen_return_value` slots. The
/// suspend value returned by the fiber primitives is always the unused null
/// channel (generators yield through `gen_last_value`), so it is released.
pub(crate) fn emit_gen_accessors(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_gen_accessors_x86_64(emitter);
        emit_gen_delegate_x86_64(emitter);
        return;
    }
    emit_gen_current_arm64(emitter);
    emit_gen_key_arm64(emitter);
    emit_gen_valid_arm64(emitter);
    emit_gen_next_arm64(emitter);
    emit_gen_send_arm64(emitter);
    emit_gen_throw_arm64(emitter);
    emit_gen_rewind_arm64(emitter);
    emit_gen_get_return_arm64(emitter);
    emit_gen_delegate_arm64(emitter);
}

// ── ARM64 accessor prologue/epilogue helpers ──────────────────────────────

/// Emits the ARM64 accessor prologue: 32-byte frame, save fp/lr + x19, cache
/// the generator pointer (argument x0) in x19.
fn arm64_acc_prologue1(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #32");                                     // reserve frame plus a saved-x19 slot
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("str x19, [sp]");                                       // preserve callee-saved x19 (generator pointer cache)
    emitter.instruction("add x29, sp, #16");                                    // anchor the new frame pointer
    emitter.instruction("mov x19, x0");                                         // x19 = generator object pointer
}

/// Emits the matching ARM64 accessor epilogue for `arm64_acc_prologue1`.
fn arm64_acc_epilogue1(emitter: &mut Emitter) {
    emitter.instruction("ldr x19, [sp]");                                       // restore caller's x19
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the ARM64 accessor prologue saving fp/lr + x19/x20 (48-byte frame),
/// caching the generator pointer in x19 and a second argument in x20.
fn arm64_acc_prologue2(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #48");                                     // reserve frame plus saved x19/x20 slots
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("stp x19, x20, [sp, #16]");                             // preserve callee-saved x19/x20
    emitter.instruction("add x29, sp, #32");                                    // anchor the new frame pointer
    emitter.instruction("mov x19, x0");                                         // x19 = generator object pointer
    emitter.instruction("mov x20, x1");                                         // x20 = second argument (sent value or Throwable)
}

/// Emits the matching ARM64 accessor epilogue for `arm64_acc_prologue2`.
fn arm64_acc_epilogue2(emitter: &mut Emitter) {
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore caller's x19/x20
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the ARM64 lazy-start guard: if the generator (in x19) has not started,
/// switch into it once to run up to its first yield, then release the unused
/// suspend value returned by `__rt_fiber_start`. `tag` makes the local label
/// unique per helper.
fn arm64_ensure_started(emitter: &mut Emitter, tag: &str) {
    let started = format!("__rt_gen_{}_started", tag);
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = generator coroutine state
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_NOT_STARTED));      // has the generator been started yet?
    emitter.instruction(&format!("b.ne {}", started));                          // skip the lazy start when already running/suspended/terminated
    emitter.instruction("mov x0, x19");                                         // pass the generator coroutine to the starter
    emitter.instruction("bl __rt_fiber_start");                                 // run the body up to its first yield (or termination)
    emitter.instruction("bl __rt_decref_mixed");                                // release the unused suspend value (generators yield via gen_last_value)
    emitter.label(&started);
}

/// `__rt_gen_current(gen) -> mixed` — lazily start, then return an owned copy
/// of the most recently yielded value.
fn emit_gen_current_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_current ---");
    emitter.label_global("__rt_gen_current");
    arm64_acc_prologue1(emitter);
    arm64_ensure_started(emitter, "current");
    emitter.instruction(&format!("ldr x0, [x19, #{}]", GEN_LAST_VALUE_OFFSET)); // load the boxed most-recent yield value
    emitter.instruction("bl __rt_incref");                                      // hand the caller an owned reference
    arm64_acc_epilogue1(emitter);
}

/// `__rt_gen_key(gen) -> mixed` — lazily start, then return an owned copy of the
/// most recently yielded key.
fn emit_gen_key_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_key ---");
    emitter.label_global("__rt_gen_key");
    arm64_acc_prologue1(emitter);
    arm64_ensure_started(emitter, "key");
    emitter.instruction(&format!("ldr x0, [x19, #{}]", GEN_LAST_KEY_OFFSET));   // load the boxed most-recent yield key
    emitter.instruction("bl __rt_incref");                                      // hand the caller an owned reference
    arm64_acc_epilogue1(emitter);
}

/// `__rt_gen_valid(gen) -> bool` — lazily start, then report whether the
/// generator has not yet terminated.
fn emit_gen_valid_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_valid ---");
    emitter.label_global("__rt_gen_valid");
    arm64_acc_prologue1(emitter);
    arm64_ensure_started(emitter, "valid");
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = current generator state
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_TERMINATED));       // has the generator finished?
    emitter.instruction("cset x0, ne");                                         // x0 = 1 while the generator can still produce values
    arm64_acc_epilogue1(emitter);
}

/// `__rt_gen_next(gen)` — lazily start, or resume a suspended generator with a
/// null sent value, advancing it to the next yield.
fn emit_gen_next_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_next ---");
    emitter.label_global("__rt_gen_next");
    arm64_acc_prologue1(emitter);
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = generator state
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_NOT_STARTED));      // has the generator started yet?
    emitter.instruction("b.ne __rt_gen_next_resume");                           // started generators advance through resume
    emitter.instruction("mov x0, x19");                                         // pass the generator to the starter
    emitter.instruction("bl __rt_fiber_start");                                 // run the body up to its first yield
    emitter.instruction("bl __rt_decref_mixed");                                // release the unused suspend value
    emitter.instruction("b __rt_gen_next_done");                                // first start already advanced to a yield
    emitter.label("__rt_gen_next_resume");
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_SUSPENDED));        // can the generator be resumed?
    emitter.instruction("b.ne __rt_gen_next_done");                             // terminated generators are a no-op
    emitter.instruction("mov x0, x19");                                         // pass the generator to the resumer
    emitter.instruction("mov x1, #0");                                          // deliver a null sent value (plain next())
    emitter.instruction("bl __rt_fiber_resume");                                // advance to the next yield (or termination)
    emitter.instruction("bl __rt_decref_mixed");                                // release the unused suspend value
    emitter.label("__rt_gen_next_done");
    arm64_acc_epilogue1(emitter);
}

/// `__rt_gen_send(gen, value) -> mixed` — lazily start, deliver `value` to the
/// suspended yield, and return an owned copy of the next yielded value.
fn emit_gen_send_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_send ---");
    emitter.label_global("__rt_gen_send");
    arm64_acc_prologue2(emitter);
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = generator state
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_NOT_STARTED));      // not yet started?
    emitter.instruction("b.ne __rt_gen_send_resume");                           // started generators skip the implicit first start
    emitter.instruction("mov x0, x19");                                         // pass the generator to the starter
    emitter.instruction("bl __rt_fiber_start");                                 // implicitly advance to the first yield before delivering the value
    emitter.instruction("bl __rt_decref_mixed");                                // release the unused suspend value
    emitter.label("__rt_gen_send_resume");
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // reload the state after a possible implicit start
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_SUSPENDED));        // is the generator paused at a yield?
    emitter.instruction("b.ne __rt_gen_send_null");                             // terminated generators return null from send()
    emitter.instruction("mov x0, x19");                                         // pass the generator to the resumer
    emitter.instruction("mov x1, x20");                                         // deliver the sent value to the suspended yield
    emitter.instruction("bl __rt_fiber_resume");                                // resume the generator with the sent value
    emitter.instruction("bl __rt_decref_mixed");                                // release the unused suspend value
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = state after the resume
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_TERMINATED));       // did the generator finish?
    emitter.instruction("b.eq __rt_gen_send_null");                             // finished generators yield no further value
    emitter.instruction(&format!("ldr x0, [x19, #{}]", GEN_LAST_VALUE_OFFSET)); // load the freshly yielded value
    emitter.instruction("bl __rt_incref");                                      // hand the caller an owned reference
    emitter.instruction("b __rt_gen_send_done");                                // skip the null fallback
    emitter.label("__rt_gen_send_null");
    emitter.instruction("mov x0, #0");                                          // send() returns null when the generator is finished
    emitter.label("__rt_gen_send_done");
    arm64_acc_epilogue2(emitter);
}

/// `__rt_gen_throw(gen, exc) -> mixed` — lazily start, inject `exc` at the
/// suspended yield so the generator's own `try/catch` can handle it, and return
/// an owned copy of the next yielded value (or re-raise to the caller when the
/// exception escapes the generator).
fn emit_gen_throw_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_throw ---");
    emitter.label_global("__rt_gen_throw");
    arm64_acc_prologue2(emitter);
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = generator state
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_NOT_STARTED));      // not yet started?
    emitter.instruction("b.ne __rt_gen_throw_inject");                          // started generators skip the implicit first start
    emitter.instruction("mov x0, x19");                                         // pass the generator to the starter
    emitter.instruction("bl __rt_fiber_start");                                 // implicitly advance to the first yield before injecting
    emitter.instruction("bl __rt_decref_mixed");                                // release the unused suspend value
    emitter.label("__rt_gen_throw_inject");
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // reload the state after a possible implicit start
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_SUSPENDED));        // is the generator paused at a yield?
    emitter.instruction("b.ne __rt_gen_throw_null");                            // nothing to inject into a finished generator
    emitter.instruction("mov x0, x19");                                         // pass the generator to the thrower
    emitter.instruction("mov x1, x20");                                         // deliver the Throwable to inject at the yield
    emitter.instruction("bl __rt_fiber_throw");                                 // re-raise the exception inside the generator (or escape to us)
    emitter.instruction("bl __rt_decref_mixed");                                // release the unused suspend value
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = state after the injected exception was handled
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_TERMINATED));       // did the generator finish after catching?
    emitter.instruction("b.eq __rt_gen_throw_null");                            // a generator that caught and finished yields nothing more
    emitter.instruction(&format!("ldr x0, [x19, #{}]", GEN_LAST_VALUE_OFFSET)); // load the value yielded after the catch
    emitter.instruction("bl __rt_incref");                                      // hand the caller an owned reference
    emitter.instruction("b __rt_gen_throw_done");                               // skip the null fallback
    emitter.label("__rt_gen_throw_null");
    emitter.instruction("mov x0, #0");                                          // throw() returns null when the generator is finished
    emitter.label("__rt_gen_throw_done");
    arm64_acc_epilogue2(emitter);
}

/// `__rt_gen_rewind(gen)` — lazily start the generator (run to its first yield).
fn emit_gen_rewind_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_rewind ---");
    emitter.label_global("__rt_gen_rewind");
    arm64_acc_prologue1(emitter);
    arm64_ensure_started(emitter, "rewind");
    arm64_acc_epilogue1(emitter);
}

/// `__rt_gen_get_return(gen) -> mixed` — return an owned copy of the value the
/// generator body passed to `return` (stored by the generator callback).
fn emit_gen_get_return_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_get_return ---");
    emitter.label_global("__rt_gen_get_return");
    emitter.instruction(&format!("ldr x0, [x0, #{}]", GEN_RETURN_VALUE_OFFSET)); // load the boxed body return value
    emitter.instruction("b __rt_incref");                                       // tail-call incref so the caller owns a fresh reference
}

// ── x86_64 accessor prologue/epilogue helpers ─────────────────────────────

/// Emits the x86_64 accessor prologue: save rbp/r12, align the stack, cache the
/// generator pointer (rdi) in r12.
fn x86_acc_prologue1(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("push r12");                                            // preserve r12 (generator pointer cache)
    emitter.instruction("sub rsp, 8");                                          // keep the stack 16-byte aligned across nested calls
    emitter.instruction("mov r12, rdi");                                        // r12 = generator object pointer
}

/// Emits the matching x86_64 accessor epilogue for `x86_acc_prologue1`.
fn x86_acc_epilogue1(emitter: &mut Emitter) {
    emitter.instruction("add rsp, 8");                                          // release the alignment pad
    emitter.instruction("pop r12");                                             // restore caller's r12
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the x86_64 accessor prologue saving rbp/r12/r13, caching the generator
/// pointer in r12 and a second argument (rsi) in r13.
fn x86_acc_prologue2(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("push r12");                                            // preserve r12 (generator pointer cache)
    emitter.instruction("push r13");                                            // preserve r13 (second-argument cache)
    emitter.instruction("mov r12, rdi");                                        // r12 = generator object pointer
    emitter.instruction("mov r13, rsi");                                        // r13 = second argument (sent value or Throwable)
}

/// Emits the matching x86_64 accessor epilogue for `x86_acc_prologue2`.
fn x86_acc_epilogue2(emitter: &mut Emitter) {
    emitter.instruction("pop r13");                                             // restore caller's r13
    emitter.instruction("pop r12");                                             // restore caller's r12
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the x86_64 lazy-start guard (see `arm64_ensure_started`).
fn x86_ensure_started(emitter: &mut Emitter, tag: &str) {
    let started = format!("__rt_gen_{}_started", tag);
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // r10 = generator coroutine state
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_NOT_STARTED));      // has the generator been started yet?
    emitter.instruction(&format!("jne {}", started));                           // skip the lazy start when already running/suspended/terminated
    emitter.instruction("mov rdi, r12");                                        // pass the generator coroutine to the starter
    emitter.instruction("call __rt_fiber_start");                               // run the body up to its first yield (or termination)
    emitter.instruction("call __rt_decref_mixed");                              // release the unused suspend value (generators yield via gen_last_value)
    emitter.label(&started);
}

/// Emits all x86_64 `__rt_gen_*` accessor helpers (see `emit_gen_accessors`).
fn emit_gen_accessors_x86_64(emitter: &mut Emitter) {
    // current
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_current ---");
    emitter.label_global("__rt_gen_current");
    x86_acc_prologue1(emitter);
    x86_ensure_started(emitter, "current");
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", GEN_LAST_VALUE_OFFSET)); // load the boxed most-recent yield value
    emitter.instruction("call __rt_incref");                                    // hand the caller an owned reference
    x86_acc_epilogue1(emitter);

    // key
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_key ---");
    emitter.label_global("__rt_gen_key");
    x86_acc_prologue1(emitter);
    x86_ensure_started(emitter, "key");
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", GEN_LAST_KEY_OFFSET)); // load the boxed most-recent yield key
    emitter.instruction("call __rt_incref");                                    // hand the caller an owned reference
    x86_acc_epilogue1(emitter);

    // valid
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_valid ---");
    emitter.label_global("__rt_gen_valid");
    x86_acc_prologue1(emitter);
    x86_ensure_started(emitter, "valid");
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // r10 = current generator state
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_TERMINATED));       // has the generator finished?
    emitter.instruction("setne al");                                            // al = 1 while the generator can still produce values
    emitter.instruction("movzx rax, al");                                       // widen the boolean to the integer result register
    x86_acc_epilogue1(emitter);

    // next
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_next ---");
    emitter.label_global("__rt_gen_next");
    x86_acc_prologue1(emitter);
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // r10 = generator state
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_NOT_STARTED));      // has the generator started yet?
    emitter.instruction("jne __rt_gen_next_resume");                            // started generators advance through resume
    emitter.instruction("mov rdi, r12");                                        // pass the generator to the starter
    emitter.instruction("call __rt_fiber_start");                               // run the body up to its first yield
    emitter.instruction("call __rt_decref_mixed");                              // release the unused suspend value
    emitter.instruction("jmp __rt_gen_next_done");                              // first start already advanced to a yield
    emitter.label("__rt_gen_next_resume");
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_SUSPENDED));        // can the generator be resumed?
    emitter.instruction("jne __rt_gen_next_done");                              // terminated generators are a no-op
    emitter.instruction("mov rdi, r12");                                        // pass the generator to the resumer
    emitter.instruction("xor esi, esi");                                        // deliver a null sent value (plain next())
    emitter.instruction("call __rt_fiber_resume");                              // advance to the next yield (or termination)
    emitter.instruction("call __rt_decref_mixed");                              // release the unused suspend value
    emitter.label("__rt_gen_next_done");
    x86_acc_epilogue1(emitter);

    // send
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_send ---");
    emitter.label_global("__rt_gen_send");
    x86_acc_prologue2(emitter);
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // r10 = generator state
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_NOT_STARTED));      // not yet started?
    emitter.instruction("jne __rt_gen_send_resume");                            // started generators skip the implicit first start
    emitter.instruction("mov rdi, r12");                                        // pass the generator to the starter
    emitter.instruction("call __rt_fiber_start");                               // implicitly advance to the first yield before delivering the value
    emitter.instruction("call __rt_decref_mixed");                              // release the unused suspend value
    emitter.label("__rt_gen_send_resume");
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // reload the state after a possible implicit start
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_SUSPENDED));        // is the generator paused at a yield?
    emitter.instruction("jne __rt_gen_send_null");                              // terminated generators return null from send()
    emitter.instruction("mov rdi, r12");                                        // pass the generator to the resumer
    emitter.instruction("mov rsi, r13");                                        // deliver the sent value to the suspended yield
    emitter.instruction("call __rt_fiber_resume");                              // resume the generator with the sent value
    emitter.instruction("call __rt_decref_mixed");                              // release the unused suspend value
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // r10 = state after the resume
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_TERMINATED));       // did the generator finish?
    emitter.instruction("je __rt_gen_send_null");                               // finished generators yield no further value
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", GEN_LAST_VALUE_OFFSET)); // load the freshly yielded value
    emitter.instruction("call __rt_incref");                                    // hand the caller an owned reference
    emitter.instruction("jmp __rt_gen_send_done");                              // skip the null fallback
    emitter.label("__rt_gen_send_null");
    emitter.instruction("xor eax, eax");                                        // send() returns null when the generator is finished
    emitter.label("__rt_gen_send_done");
    x86_acc_epilogue2(emitter);

    // throw
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_throw ---");
    emitter.label_global("__rt_gen_throw");
    x86_acc_prologue2(emitter);
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // r10 = generator state
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_NOT_STARTED));      // not yet started?
    emitter.instruction("jne __rt_gen_throw_inject");                           // started generators skip the implicit first start
    emitter.instruction("mov rdi, r12");                                        // pass the generator to the starter
    emitter.instruction("call __rt_fiber_start");                               // implicitly advance to the first yield before injecting
    emitter.instruction("call __rt_decref_mixed");                              // release the unused suspend value
    emitter.label("__rt_gen_throw_inject");
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // reload the state after a possible implicit start
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_SUSPENDED));        // is the generator paused at a yield?
    emitter.instruction("jne __rt_gen_throw_null");                             // nothing to inject into a finished generator
    emitter.instruction("mov rdi, r12");                                        // pass the generator to the thrower
    emitter.instruction("mov rsi, r13");                                        // deliver the Throwable to inject at the yield
    emitter.instruction("call __rt_fiber_throw");                               // re-raise the exception inside the generator (or escape to us)
    emitter.instruction("call __rt_decref_mixed");                              // release the unused suspend value
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // r10 = state after the injected exception was handled
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_TERMINATED));       // did the generator finish after catching?
    emitter.instruction("je __rt_gen_throw_null");                              // a generator that caught and finished yields nothing more
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", GEN_LAST_VALUE_OFFSET)); // load the value yielded after the catch
    emitter.instruction("call __rt_incref");                                    // hand the caller an owned reference
    emitter.instruction("jmp __rt_gen_throw_done");                             // skip the null fallback
    emitter.label("__rt_gen_throw_null");
    emitter.instruction("xor eax, eax");                                        // throw() returns null when the generator is finished
    emitter.label("__rt_gen_throw_done");
    x86_acc_epilogue2(emitter);

    // rewind
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_rewind ---");
    emitter.label_global("__rt_gen_rewind");
    x86_acc_prologue1(emitter);
    x86_ensure_started(emitter, "rewind");
    x86_acc_epilogue1(emitter);

    // getReturn
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_get_return ---");
    emitter.label_global("__rt_gen_get_return");
    emitter.instruction(&format!("mov rax, QWORD PTR [rdi + {}]", GEN_RETURN_VALUE_OFFSET)); // load the boxed body return value
    emitter.instruction("jmp __rt_incref");                                     // tail-call incref so the caller owns a fresh reference
}

/// `__rt_gen_delegate(inner) -> mixed` — drives an inner Generator on behalf of
/// `yield from`, running on the *outer* generator's coroutine stack.
///
/// Lazily starts the inner generator, then repeatedly yields the inner's current
/// key/value out through the outer generator's suspend boundary
/// (`__rt_gen_suspend`, which targets `_fiber_current` = the outer generator) and
/// forwards each resumed sent value back into the inner generator
/// (`__rt_fiber_resume`). When the inner generator terminates, returns an owned
/// copy of its `return` value (the value of the `yield from` expression). The
/// `inner` argument is borrowed; the caller retains ownership.
fn emit_gen_delegate_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_delegate ---");
    emitter.label_global("__rt_gen_delegate");
    emitter.instruction("sub sp, sp, #48");                                     // reserve frame plus saved callee registers
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("stp x19, x20, [sp, #16]");                             // preserve callee-saved x19/x20
    emitter.instruction("add x29, sp, #32");                                    // anchor the frame pointer
    emitter.instruction("mov x19, x0");                                         // x19 = inner generator object (borrowed)

    // -- lazily start the inner generator so its first yield is available --
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = inner generator state
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_NOT_STARTED));      // has the inner generator started yet?
    emitter.instruction("b.ne __rt_gen_delegate_loop");                         // skip the lazy start when already advanced
    emitter.instruction("mov x0, x19");                                         // pass the inner generator to the starter
    emitter.instruction("bl __rt_fiber_start");                                 // run the inner body up to its first yield
    emitter.instruction("bl __rt_decref_mixed");                                // release the unused inner suspend value

    emitter.label("__rt_gen_delegate_loop");
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // x9 = inner generator state
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_TERMINATED));       // has the inner generator finished?
    emitter.instruction("b.eq __rt_gen_delegate_done");                         // stop delegating once the inner generator is exhausted
    // -- yield the inner key/value out through the outer suspend boundary --
    emitter.instruction(&format!("ldr x0, [x19, #{}]", GEN_LAST_KEY_OFFSET));   // x0 = inner generator's current key cell
    emitter.instruction("bl __rt_incref");                                      // own a key reference for the outer suspend (it consumes one)
    emitter.instruction("mov x20, x0");                                         // stash the owned key while boxing the value
    emitter.instruction(&format!("ldr x0, [x19, #{}]", GEN_LAST_VALUE_OFFSET)); // x0 = inner generator's current value cell
    emitter.instruction("bl __rt_incref");                                      // own a value reference for the outer suspend (it consumes one)
    emitter.instruction("mov x1, x0");                                          // value -> suspend argument 1
    emitter.instruction("mov x0, x20");                                         // key -> suspend argument 0
    emitter.instruction("bl __rt_gen_suspend");                                 // suspend the outer generator; x0 = sent value (owned) on resume
    emitter.instruction("mov x20, x0");                                         // stash the sent value for forwarding
    // -- forward the sent value into the inner generator, advancing it --
    emitter.instruction(&format!("ldr x9, [x19, #{}]", FIBER_STATE_OFFSET));    // reload the inner state before resuming
    emitter.instruction(&format!("cmp x9, #{}", FIBER_STATE_SUSPENDED));        // can the inner generator accept the sent value?
    emitter.instruction("b.ne __rt_gen_delegate_drop");                         // drop the sent value when the inner generator cannot resume
    emitter.instruction("mov x0, x19");                                         // pass the inner generator to the resumer
    emitter.instruction("mov x1, x20");                                         // forward the sent value into the inner generator
    emitter.instruction("bl __rt_fiber_resume");                                // advance the inner generator to its next yield (or termination)
    emitter.instruction("bl __rt_decref_mixed");                                // release the unused inner suspend value
    emitter.instruction("b __rt_gen_delegate_loop");                            // continue delegating to the inner generator
    emitter.label("__rt_gen_delegate_drop");
    emitter.instruction("mov x0, x20");                                         // move the undeliverable sent value into the releaser
    emitter.instruction("bl __rt_decref_mixed");                                // release the sent value the inner generator cannot accept
    emitter.instruction("b __rt_gen_delegate_loop");                            // re-check the inner state and finish delegating

    emitter.label("__rt_gen_delegate_done");
    emitter.instruction(&format!("ldr x0, [x19, #{}]", GEN_RETURN_VALUE_OFFSET)); // x0 = inner generator's return value cell
    emitter.instruction("bl __rt_incref");                                      // hand the caller an owned reference to the delegated return value
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore callee-saved x19/x20
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the frame
    emitter.instruction("ret");                                                 // return the delegated return value to the body
}


/// x86_64 counterpart of `emit_gen_delegate_arm64` (see that function).
fn emit_gen_delegate_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_delegate ---");
    emitter.label_global("__rt_gen_delegate");
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // anchor the frame pointer
    emitter.instruction("push r12");                                            // preserve r12 (inner generator cache)
    emitter.instruction("push r13");                                            // preserve r13 (key/sent scratch)
    emitter.instruction("mov r12, rdi");                                        // r12 = inner generator object (borrowed)

    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // r10 = inner generator state
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_NOT_STARTED));      // has the inner generator started yet?
    emitter.instruction("jne __rt_gen_delegate_loop");                          // skip the lazy start when already advanced
    emitter.instruction("mov rdi, r12");                                        // pass the inner generator to the starter
    emitter.instruction("call __rt_fiber_start");                               // run the inner body up to its first yield (suspend value left in rax)
    emitter.instruction("call __rt_decref_mixed");                              // release the unused inner suspend value

    emitter.label("__rt_gen_delegate_loop");
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // r10 = inner generator state
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_TERMINATED));       // has the inner generator finished?
    emitter.instruction("je __rt_gen_delegate_done");                           // stop delegating once the inner generator is exhausted
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", GEN_LAST_KEY_OFFSET)); // rax = inner generator's current key cell
    emitter.instruction("call __rt_incref");                                    // own a key reference for the outer suspend
    emitter.instruction("mov r13, rax");                                        // stash the owned key while fetching the value
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", GEN_LAST_VALUE_OFFSET)); // rax = inner generator's current value cell
    emitter.instruction("call __rt_incref");                                    // own a value reference for the outer suspend
    emitter.instruction("mov rsi, rax");                                        // value -> suspend argument 1
    emitter.instruction("mov rdi, r13");                                        // key -> suspend argument 0
    emitter.instruction("call __rt_gen_suspend");                               // suspend the outer generator; rax = sent value on resume
    emitter.instruction("mov r13, rax");                                        // stash the sent value for forwarding
    emitter.instruction(&format!("mov r10, QWORD PTR [r12 + {}]", FIBER_STATE_OFFSET)); // reload the inner state before resuming
    emitter.instruction(&format!("cmp r10, {}", FIBER_STATE_SUSPENDED));        // can the inner generator accept the sent value?
    emitter.instruction("jne __rt_gen_delegate_drop");                          // drop the sent value when the inner generator cannot resume
    emitter.instruction("mov rdi, r12");                                        // pass the inner generator to the resumer
    emitter.instruction("mov rsi, r13");                                        // forward the sent value into the inner generator
    emitter.instruction("call __rt_fiber_resume");                              // advance the inner generator to its next yield
    emitter.instruction("call __rt_decref_mixed");                              // release the unused inner suspend value
    emitter.instruction("jmp __rt_gen_delegate_loop");                          // continue delegating to the inner generator
    emitter.label("__rt_gen_delegate_drop");
    emitter.instruction("mov rax, r13");                                        // move the undeliverable sent value into the releaser
    emitter.instruction("call __rt_decref_mixed");                              // release the sent value the inner generator cannot accept
    emitter.instruction("jmp __rt_gen_delegate_loop");                          // re-check the inner state and finish delegating

    emitter.label("__rt_gen_delegate_done");
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", GEN_RETURN_VALUE_OFFSET)); // rax = inner generator's return value cell
    emitter.instruction("call __rt_incref");                                    // hand the caller an owned reference to the delegated return value
    emitter.instruction("pop r13");                                             // restore callee-saved r13
    emitter.instruction("pop r12");                                             // restore callee-saved r12
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the delegated return value to the body
}
