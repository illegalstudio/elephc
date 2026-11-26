//! Purpose:
//! Publishes owner-slot replacements before retiring displaced values.
//!
//! Called from:
//! - Compiler frame, reference, and symbol storage, eval writeback, and shutdown.
//!
//! Key details:
//! - The replacement is visible before cleanup can run destructors or reenter the same symbol.
//! - Consumers select semantic marker updates and the release helper for their owner type.

use crate::{abi, Arch, Emitter};

/// Stages the old owner, publishes the replacement, and returns the old owner in the result register.
///
/// Publication may update multiple words and clobber scratch registers. It must
/// finish before retirement can invoke PHP, and must not invoke PHP itself.
pub fn emit_publish_owner<E, T>(
    emitter: &mut E,
    load_previous: impl FnOnce(&mut E, &str),
    publish: impl FnOnce(&mut E) -> T,
) -> T
where
    E: Emitter,
{
    let previous = abi::secondary_scratch_reg(emitter);
    emit_publish_owner_using(emitter, previous, load_previous, publish)
}

/// Stages a displaced address-backed owner without borrowing the address register.
pub fn emit_publish_address_owner<E: Emitter, T>(
    emitter: &mut E, address: &str, offset: usize, publish: impl FnOnce(&mut E) -> T,
) -> T {
    let mut previous = abi::secondary_scratch_reg(emitter);
    if previous == address {
        previous = abi::tertiary_scratch_reg(emitter);
    }
    emit_publish_owner_using(emitter, previous,
        |emitter, previous| abi::emit_load_from_address(emitter, previous, address, offset),
        publish,
    )
}

/// Preserves the loaded predecessor across all writes belonging to the publication.
fn emit_publish_owner_using<E: Emitter, T>(
    emitter: &mut E,
    previous: &str,
    load_previous: impl FnOnce(&mut E, &str),
    publish: impl FnOnce(&mut E) -> T,
) -> T {
    load_previous(emitter, previous);
    abi::emit_push_reg(emitter, previous);
    let published = publish(emitter);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
    published
}

/// Publishes an owner through a caller-selected local or reference address.
///
/// Both callbacks use the prepared value in its ABI result registers. The
/// predecessor is returned in the integer result register when publication ends.
pub fn emit_publish_staged_owner<E, S, Err>(
    state: &mut S,
    emitter: fn(&mut S) -> &mut E,
    registers: &[&str],
    load_previous: impl FnOnce(&mut S) -> Result<(), Err>,
    publish: impl FnOnce(&mut S) -> Result<(), Err>,
) -> Result<(), Err>
where
    E: Emitter,
{
    debug_assert!((1..=2).contains(&registers.len()));
    emitter(state).emit_reserve_temporary_stack(16);
    for (index, register) in registers.iter().enumerate() {
        emitter(state).emit_store_to_sp(register, index * 8);
    }
    load_previous(state)?;
    emitter(state).emit_push_reg(registers[0]);
    for (index, register) in registers.iter().enumerate() {
        emitter(state).emit_load_temporary_stack_slot(register, 16 + index * 8);
    }
    publish(state)?;
    let emitter = emitter(state);
    emitter.emit_pop_reg(emitter.int_result_reg());
    emitter.emit_release_temporary_stack(16);
    Ok(())
}

/// Continues stabilization after a later root's destructor repopulates an earlier slot.
pub fn emit_branch_if_published_owner(
    emitter: &mut impl Emitter, symbol: &str, restart: &str, empty: &str,
) {
    let result = abi::int_result_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
    crate::layout::emit_branch_if_null_container(
        emitter, result, abi::secondary_scratch_reg(emitter), empty,
    );
    abi::emit_jump(emitter, restart);
    emitter.label(empty);
}

/// Replaces the owner in `symbol`, preserves the published result, then retires its predecessor.
pub fn emit_replace_result_owner<E, P, R>(
    emitter: &mut E,
    symbol: &str,
    after_publish: P,
    release_previous: R,
)
where
    E: Emitter,
    P: FnOnce(&mut E),
    R: FnOnce(&mut E),
{
    let result = abi::int_result_reg(emitter);
    let previous = abi::secondary_scratch_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, previous, symbol, 0);
    abi::emit_push_reg(emitter, previous);
    abi::emit_store_reg_to_symbol(emitter, result, symbol, 0);
    abi::emit_push_reg(emitter, result);
    after_publish(emitter);
    emitter.emit_load_temporary_stack_slot(result, 16);
    release_previous(emitter);
    abi::emit_pop_reg(emitter, result);
    emitter.emit_release_temporary_stack(16);
}

/// Runtime representation whose destructor must see its root still published at shutdown.
#[derive(Clone, Copy)]
pub enum PublishedDestructor {
    /// Release can run nested callbacks, but there is no direct object receiver.
    None,
    /// The root directly owns an object pointer.
    Object,
    /// The root owns a Mixed cell which may contain an object.
    Mixed,
}

/// Drains one persistent root and all replacements published by its cleanup callbacks.
///
/// An object destructor may inspect the root that made it reachable. Keep an
/// extra lease and invoke it while the root is live. Compare against the pinned
/// owner before clearing storage so a reentrant replacement wins. The caller
/// supplies storage markers and a protected release compatible with its scope.
pub fn emit_drain_symbol_owner<E: Emitter>(
    emitter: &mut E,
    symbol: &str,
    label: &str,
    destructor: PublishedDestructor,
    mut clear: impl FnMut(&mut E),
    mut release: impl FnMut(&mut E),
) {
    let done = format!("{label}_empty");
    let release_pin = format!("{label}_release_pin");
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.label(label);
    abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
    crate::layout::emit_branch_if_null_container(emitter, result, scratch, &done);
    if matches!(destructor, PublishedDestructor::None) {
        clear(emitter);
    } else {
        abi::emit_push_reg(emitter, result);
        abi::emit_call_label(emitter, "__rt_incref");
        emitter.emit_load_temporary_stack_slot(result, 0);
        emit_published_destructor(emitter, destructor, label);
        emitter.emit_load_temporary_stack_slot(result, 0);
        abi::emit_cmp_reg_to_symbol(emitter, result, symbol);
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.instruction(&format!("b.ne {release_pin}"));            // keep a newer publication installed by the destructor
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("jne {release_pin}"));             // keep a newer publication installed by the destructor
            }
        }
        clear(emitter);
        release(emitter);
        emitter.label(&release_pin);
        abi::emit_pop_reg(emitter, result);
    }
    release(emitter);
    abi::emit_jump(emitter, label);
    emitter.label(&done);
    clear(emitter);
}

/// Calls a direct object destructor at most once while the published owner is pinned.
fn emit_published_destructor(
    emitter: &mut impl Emitter, destructor: PublishedDestructor, label: &str,
) {
    let skip = format!("{label}_skip_destructor");
    let result = abi::int_result_reg(emitter);
    let receiver = abi::int_arg_reg_name(emitter, 0);
    if matches!(destructor, PublishedDestructor::Mixed) {
        abi::emit_call_label(emitter, "__rt_mixed_unbox");
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #6");                              // run a destructor only for the boxed object tag
                emitter.instruction(&format!("b.ne {skip}"));                   // leave other payloads to their normal release
                emitter.instruction("mov x0, x1");                              // use the borrowed raw object owned by the pinned box
            }
            Arch::X86_64 => {
                emitter.instruction("cmp rax, 6");                              // run a destructor only for the boxed object tag
                emitter.instruction(&format!("jne {skip}"));                    // leave other payloads to their normal release
            }
        }
    } else {
        abi::emit_reg_move(emitter, receiver, result);
    }
    crate::layout::emit_branch_if_null_container(
        emitter, receiver, abi::secondary_scratch_reg(emitter), &skip,
    );
    crate::values::destructor_header::emit_skip_running(emitter, &skip);
    abi::emit_push_reg(emitter, receiver);
    abi::emit_reg_move(emitter, result, receiver);
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.emit_load_temporary_stack_slot(receiver, 0);
    crate::cleanup_preserve::emit_unary_cleanup_preserving_exception(
        emitter, "__rt_call_object_destructor", receiver,
    );
    abi::emit_pop_reg(emitter, receiver);
    crate::values::destructor_header::emit_clear_running(emitter);
    crate::cleanup_preserve::emit_unary_cleanup_preserving_exception(
        emitter, "__rt_decref_object", receiver,
    );
    emitter.label(&skip);
}
