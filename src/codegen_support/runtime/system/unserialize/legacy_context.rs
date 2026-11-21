//! Purpose:
//! Preserves policies and reference owners when Serializable payloads share a parser registry.
//!
//! Called from:
//! - Unserialize begin/end and builtin lowering for nested calls without explicit options.
//!
//! Key details:
//! - A saved count of -1 marks a shared registry; ordinary logical counts are nonnegative.
//! - Nested shared owners move to the enclosing context before its cleanup list is detached.

use crate::codegen_support::{abi, emit::Emitter};

/// Emits legacy policy inheritance and reference-owner promotion helpers on every target.
pub(super) fn emit(emitter: &mut Emitter) {
    emit_inherit_policy(emitter);
    emit_promote_data(emitter);
}

/// Inherits the enclosing class policy only when the nested call omitted its options argument.
fn emit_inherit_policy(emitter: &mut Emitter) {
    // -- inherit and retain the outer policy for shared calls without explicit options --
    emitter.blank();
    emitter.label_global("__rt_unserialize_inherit_policy");
    abi::emit_frame_prologue(emitter, 32);
    super::context::SNAPSHOT.emit_inherit_shared_owner(emitter, "__rt_unserialize_inherit_policy_done");
    emitter.label("__rt_unserialize_inherit_policy_done");
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

/// Keeps nested registry leases alive until the enclosing shared parser completes.
fn emit_promote_data(emitter: &mut Emitter) {
    // -- demote the newest reference leases to the enclosing parser depth --
    emitter.blank();
    emitter.label_global("__rt_unserialize_promote_legacy_data");
    super::context::SNAPSHOT.emit_branch_unless_shared(emitter, "__rt_unserialize_promote_legacy_done");
    elephc_memoir::scoped_owners::emit_promote_prefix(
        emitter, super::temporaries::TEMPORARIES,
        "__rt_unserialize_promote_legacy_loop", "__rt_unserialize_promote_legacy_done",
    );
    emitter.label("__rt_unserialize_promote_legacy_done");
    abi::emit_return(emitter);
}
