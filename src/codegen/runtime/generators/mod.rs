//! Purpose:
//! Emits the `__rt_gen_*` runtime helpers backing the `Generator` built-in
//! class: `current`, `key`, `valid`, `next`, `send`, `rewind`, `throw`,
//! `getReturn`.
//!
//! Called from:
//!  - `crate::codegen::runtime::emitters::emit_runtime()` (once per build).
//!  - User PHP method calls on `Generator` are routed here by the dispatcher
//!    intercept in `crate::codegen::expr::objects::dispatch::vtable`.
//!
//! Key details:
//!  - Each helper takes a `GeneratorFrame*` in `x0` and reads/writes the
//!    layout defined in `frame.rs`. The contract is shared with the
//!    state-machine resume function emitted by `codegen::functions::generator`.
//!  - Yield codegen pre-boxes Mixed values via `__rt_mixed_from_value` and
//!    stores them in frame slots, so `current`/`key`/`getReturn` are pure
//!    load-and-return paths with no allocation.

pub(crate) mod frame;

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

use frame as f;

pub(crate) fn emit_generator_runtime(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_generator_runtime_x86_64(emitter);
        return;
    }

    emit_gen_current(emitter);
    emit_gen_key(emitter);
    emit_gen_valid(emitter);
    emit_gen_next(emitter);
    emit_gen_send(emitter);
    emit_gen_rewind(emitter);
    emit_gen_throw(emitter);
    emit_gen_get_return(emitter);
}

fn emit_generator_runtime_x86_64(emitter: &mut Emitter) {
    emit_gen_current_x86_64(emitter);
    emit_gen_key_x86_64(emitter);
    emit_gen_valid_x86_64(emitter);
    emit_gen_next_x86_64(emitter);
    emit_gen_send_x86_64(emitter);
    emit_gen_rewind_x86_64(emitter);
    emit_gen_throw_x86_64(emitter);
    emit_gen_get_return_x86_64(emitter);
}

/// `current(): mixed` — returns the boxed Mixed pointer stashed by the
/// most recent yield. The yield codegen already called
/// `__rt_mixed_from_value` to materialize the cell; we incref it here so
/// the caller receives an *owned* reference whose lifetime is independent
/// of the generator's frame slot. Without this retain, the next yield's
/// `emit_replace_mixed_slot` call decrefs the cell down to zero and frees
/// it while the caller still holds the pointer (e.g. inside a `foreach
/// ($g as $k => $v)` body).
fn emit_gen_current(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_current ---");
    emitter.label_global("__rt_gen_current");
    emitter.instruction("sub sp, sp, #32");                                     // reserve a helper frame while an initial current() may resume the generator
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address across the resume call
    emitter.instruction("str x19, [sp, #0]");                                   // preserve caller's x19 before caching the generator frame
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer
    emitter.instruction("mov x19, x0");                                         // x19 = generator frame pointer
    emitter.instruction(&format!("ldr w1, [x19, #{}]", f::OFF_FLAGS));          // load generator flags
    emitter.instruction(&format!("tbnz w1, #0, __rt_gen_current_load"));        // skip auto-rewind once the generator has already started
    emitter.instruction(&format!("orr w1, w1, #{}", f::FLAG_REWOUND));          // mark current() as the first rewind/start operation
    emitter.instruction(&format!("str w1, [x19, #{}]", f::OFF_FLAGS));          // persist the rewound flag before entering user code
    emitter.instruction(&format!("ldr x9, [x19, #{}]", f::OFF_RESUME_FN));      // load the resume function pointer
    emitter.instruction("mov x0, x19");                                         // pass the generator frame to the resume function
    emitter.instruction("blr x9");                                              // run until the first yield so current() matches PHP's lazy start
    emitter.label("__rt_gen_current_load");
    emitter.instruction(&format!("ldr x0, [x19, #{}]", f::OFF_LAST_VALUE));     // load the boxed Mixed pointer for the most-recent yield value
    emitter.instruction("bl __rt_incref");                                      // incref so the caller owns a fresh refcount on the cell
    emitter.instruction("ldr x19, [sp, #0]");                                   // restore caller's x19
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the owned Mixed pointer from current()
}

fn emit_gen_key(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_key ---");
    emitter.label_global("__rt_gen_key");
    emitter.instruction(&format!("ldr x0, [x0, #{}]", f::OFF_LAST_KEY));        // load the boxed Mixed pointer for the most-recent yield key
    emitter.instruction("b __rt_incref");                                       // tail-call incref so the caller owns a fresh refcount on the cell
}

/// `valid(): bool` — returns 1 unless the TERMINATED flag is set.
fn emit_gen_valid(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_valid ---");
    emitter.label_global("__rt_gen_valid");
    emitter.instruction(&format!("ldr w1, [x0, #{}]", f::OFF_FLAGS));           // load flags word
    emitter.instruction(&format!("tst w1, #{}", f::FLAG_TERMINATED));           // test the TERMINATED bit
    emitter.instruction("cset x0, eq");                                         // x0 = 1 if not terminated, else 0
    emitter.instruction("ret");                                                 // return the validity flag
}

/// `next(): void` — advances the generator past the current yield. If
/// already terminated, no-op.
fn emit_gen_next(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_next ---");
    emitter.label_global("__rt_gen_next");
    emitter.instruction(&format!("ldr w1, [x0, #{}]", f::OFF_FLAGS));           // load flags
    emitter.instruction(&format!("tbnz w1, #1, __rt_gen_next_done"));           // if TERMINATED bit set, skip the resume call
    emitter.instruction(&format!("ldr x9, [x0, #{}]", f::OFF_RESUME_FN));       // load the resume function pointer
    emitter.instruction("br x9");                                               // tail-call resume_fn(x0=frame)
    emitter.label_global("__rt_gen_next_done");
    emitter.instruction("ret");                                                 // already terminated — return immediately
}

/// `send($value): mixed` — stash the caller-owned boxed Mixed payload in the
/// frame's `sent_value` slot, then resume the body. The yield-assignment
/// lowering consumes and clears that box on the resume path.
fn emit_gen_send(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_send ---");
    emitter.label_global("__rt_gen_send");
    emitter.instruction(&format!("ldr w2, [x0, #{}]", f::OFF_FLAGS));           // load flags
    emitter.instruction(&format!("tbnz w2, #1, __rt_gen_send_done"));           // if TERMINATED bit set, skip resume
    emitter.instruction(&format!("str x1, [x0, #{}]", f::OFF_SENT_VALUE));      // stash the boxed sent payload in sent_value
    emitter.instruction(&format!("ldr x9, [x0, #{}]", f::OFF_RESUME_FN));       // load resume function pointer
    emitter.instruction("br x9");                                               // tail-call resume_fn (returns whatever current() now reflects)
    emitter.label_global("__rt_gen_send_done");
    emitter.instruction("mov x0, #0");                                          // return null when terminated
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_gen_rewind(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_rewind ---");
    emitter.label_global("__rt_gen_rewind");
    emitter.instruction(&format!("ldr w1, [x0, #{}]", f::OFF_FLAGS));           // load flags
    emitter.instruction(&format!("tbnz w1, #0, __rt_gen_rewind_done"));         // if REWOUND bit set, skip
    emitter.instruction(&format!("orr w1, w1, #{}", f::FLAG_REWOUND));          // set REWOUND bit
    emitter.instruction(&format!("str w1, [x0, #{}]", f::OFF_FLAGS));           // store updated flags
    emitter.instruction(&format!("ldr x9, [x0, #{}]", f::OFF_RESUME_FN));       // load resume_fn
    emitter.instruction("br x9");                                               // tail-call resume_fn(x0=frame)
    emitter.label_global("__rt_gen_rewind_done");
    emitter.instruction("ret");                                                 // already rewound — return immediately
}

/// `throw($exc)` — inject an exception that propagates up the caller's
/// stack as if the generator had thrown it. v1 always terminates the
/// generator (yield inside try/catch is rejected at type-check time, so
/// the body cannot catch it). The exception is published in the global
/// `_exc_value` slot and `__rt_throw_current` performs the longjmp into
/// the caller's nearest active handler.
fn emit_gen_throw(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_throw ---");
    emitter.label_global("__rt_gen_throw");
    // Mark the generator as terminated so any subsequent valid()/next() is a no-op.
    emitter.instruction(&format!("ldr w2, [x0, #{}]", f::OFF_FLAGS));           // load flags
    emitter.instruction(&format!("orr w2, w2, #{}", f::FLAG_TERMINATED));       // set TERMINATED bit
    emitter.instruction(&format!("str w2, [x0, #{}]", f::OFF_FLAGS));           // store updated flags
    // Publish the exception object pointer (in x1, the first ABI argument
    // after the receiver) in the global "active exception" slot.
    crate::codegen::abi::emit_store_reg_to_symbol(emitter, "x1", "_exc_value", 0);
    emitter.instruction("b __rt_throw_current");                                // tail-call the unwinder; never returns
}

fn emit_gen_get_return(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_get_return ---");
    emitter.label_global("__rt_gen_get_return");
    emitter.instruction(&format!("ldr x0, [x0, #{}]", f::OFF_RETURN_VALUE));    // load the boxed return_value pointer
    emitter.instruction("b __rt_incref");                                       // tail-call incref so the caller owns a fresh refcount on the cell
}

fn emit_gen_current_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_current ---");
    emitter.label_global("__rt_gen_current");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer while current() may resume user code
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame
    emitter.instruction("push r12");                                            // preserve r12 before caching the generator frame
    emitter.instruction("sub rsp, 8");                                          // keep the stack 16-byte aligned across nested calls
    emitter.instruction("mov r12, rdi");                                        // r12 = generator frame pointer
    emitter.instruction(&format!("mov r10d, DWORD PTR [r12 + {}]", f::OFF_FLAGS)); // load generator flags
    emitter.instruction("test r10d, 1");                                        // has the generator already been rewound/started?
    emitter.instruction("jnz __rt_gen_current_load");                           // skip auto-rewind when current() is not the first operation
    emitter.instruction(&format!("or r10d, {}", f::FLAG_REWOUND));              // mark current() as the first rewind/start operation
    emitter.instruction(&format!("mov DWORD PTR [r12 + {}], r10d", f::OFF_FLAGS)); // persist the rewound flag before entering user code
    emitter.instruction("mov rdi, r12");                                        // pass the generator frame to the resume function
    emitter.instruction(&format!("call QWORD PTR [r12 + {}]", f::OFF_RESUME_FN)); // run until the first yield so current() matches PHP's lazy start
    emitter.label("__rt_gen_current_load");
    emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", f::OFF_LAST_VALUE)); // load the boxed Mixed pointer for the most-recent yield value
    emitter.instruction("call __rt_incref");                                    // incref so the caller owns a fresh refcount on the cell
    emitter.instruction("add rsp, 8");                                          // release the alignment pad
    emitter.instruction("pop r12");                                             // restore caller's r12
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return the owned Mixed pointer from current()
}

fn emit_gen_key_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_key ---");
    emitter.label_global("__rt_gen_key");
    emitter.instruction(&format!("mov rax, QWORD PTR [rdi + {}]", f::OFF_LAST_KEY)); // load the boxed Mixed pointer for the most-recent yield key
    emitter.instruction("jmp __rt_incref");                                     // tail-call incref so the caller owns a fresh refcount on the cell
}

fn emit_gen_valid_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_valid ---");
    emitter.label_global("__rt_gen_valid");
    emitter.instruction(&format!("mov r10d, DWORD PTR [rdi + {}]", f::OFF_FLAGS)); // load flags word
    emitter.instruction(&format!("test r10d, {}", f::FLAG_TERMINATED));         // test the TERMINATED bit
    emitter.instruction("sete al");                                             // al = 1 if not terminated, else 0
    emitter.instruction("movzx rax, al");                                       // widen the validity flag to the integer return register
    emitter.instruction("ret");                                                 // return the validity flag
}

fn emit_gen_next_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_next ---");
    emitter.label_global("__rt_gen_next");
    emitter.instruction(&format!("mov r10d, DWORD PTR [rdi + {}]", f::OFF_FLAGS)); // load flags
    emitter.instruction(&format!("test r10d, {}", f::FLAG_TERMINATED));         // check whether the generator is already terminated
    emitter.instruction("jnz __rt_gen_next_done");                              // if TERMINATED bit set, skip the resume call
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", f::OFF_RESUME_FN)); // load the resume function pointer
    emitter.instruction("jmp r10");                                             // tail-call resume_fn(rdi=frame)
    emitter.label_global("__rt_gen_next_done");
    emitter.instruction("ret");                                                 // already terminated — return immediately
}

fn emit_gen_send_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_send ---");
    emitter.label_global("__rt_gen_send");
    emitter.instruction(&format!("mov r10d, DWORD PTR [rdi + {}]", f::OFF_FLAGS)); // load flags
    emitter.instruction(&format!("test r10d, {}", f::FLAG_TERMINATED));         // check whether the generator is already terminated
    emitter.instruction("jnz __rt_gen_send_done");                              // if TERMINATED bit set, skip resume
    emitter.instruction(&format!("mov QWORD PTR [rdi + {}], rsi", f::OFF_SENT_VALUE)); // stash the boxed sent payload in sent_value
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", f::OFF_RESUME_FN)); // load resume function pointer
    emitter.instruction("jmp r10");                                             // tail-call resume_fn(rdi=frame)
    emitter.label_global("__rt_gen_send_done");
    emitter.instruction("xor rax, rax");                                        // return null when terminated
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_gen_rewind_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_rewind ---");
    emitter.label_global("__rt_gen_rewind");
    emitter.instruction(&format!("mov r10d, DWORD PTR [rdi + {}]", f::OFF_FLAGS)); // load flags
    emitter.instruction(&format!("test r10d, {}", f::FLAG_REWOUND));            // check whether rewind already happened
    emitter.instruction("jnz __rt_gen_rewind_done");                            // if REWOUND bit set, skip
    emitter.instruction(&format!("or r10d, {}", f::FLAG_REWOUND));              // set REWOUND bit
    emitter.instruction(&format!("mov DWORD PTR [rdi + {}], r10d", f::OFF_FLAGS)); // store updated flags
    emitter.instruction(&format!("mov r10, QWORD PTR [rdi + {}]", f::OFF_RESUME_FN)); // load resume_fn
    emitter.instruction("jmp r10");                                             // tail-call resume_fn(rdi=frame)
    emitter.label_global("__rt_gen_rewind_done");
    emitter.instruction("ret");                                                 // already rewound — return immediately
}

fn emit_gen_throw_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_throw ---");
    emitter.label_global("__rt_gen_throw");
    emitter.instruction(&format!("mov r10d, DWORD PTR [rdi + {}]", f::OFF_FLAGS)); // load flags
    emitter.instruction(&format!("or r10d, {}", f::FLAG_TERMINATED));           // set TERMINATED bit
    emitter.instruction(&format!("mov DWORD PTR [rdi + {}], r10d", f::OFF_FLAGS)); // store updated flags
    crate::codegen::abi::emit_store_reg_to_symbol(emitter, "rsi", "_exc_value", 0);
    emitter.instruction("jmp __rt_throw_current");                              // tail-call the unwinder; never returns
}

fn emit_gen_get_return_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_gen_get_return ---");
    emitter.label_global("__rt_gen_get_return");
    emitter.instruction(&format!("mov rax, QWORD PTR [rdi + {}]", f::OFF_RETURN_VALUE)); // load the boxed return_value pointer
    emitter.instruction("jmp __rt_incref");                                     // tail-call incref so the caller owns a fresh refcount on the cell
}
